use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::models::{Encounter, SpellContext, PendingAttack, PendingSpell, LongDurationSpell};
use crate::parsing::line_parser::{ParsedLine, is_long_duration_spell, get_spell_damage_type};

pub fn process_parsed_line(
    parsed: ParsedLine,
    combat_time: u64,
    last_combat_time: &mut u64,
    current_encounter: &mut Option<u64>,
    spell_contexts: &mut Vec<SpellContext>,
    pending_attacks: &mut Vec<PendingAttack>,
    pending_spells: &mut Vec<PendingSpell>,
    long_duration_spells: &mut Vec<LongDurationSpell>,
    encounters: &Arc<Mutex<HashMap<u64, Encounter>>>,
    encounter_counter: &Arc<Mutex<u64>>
) {
    const ENCOUNTER_TIMEOUT: u64 = 6;
    
    // Only consider damage > 0 events for encounter timeout calculations
    let should_update_combat_time = match &parsed {
        ParsedLine::Damage { total, .. } => *total > 0,
        _ => false,
    };
    
    // Check if we need to start a new encounter (only based on damage > 0 events)
    let should_start_new = if should_update_combat_time {
        current_encounter.is_none() || 
        (combat_time > *last_combat_time && 
         combat_time.saturating_sub(*last_combat_time) > ENCOUNTER_TIMEOUT)
    } else {
        current_encounter.is_none()  // Only start new if no encounter exists
    };
    
    if should_start_new {
        let new_id = {
            let mut counter = encounter_counter.lock().unwrap();
            let id = *counter;
            *counter += 1;
            id
        };
        
        let new_encounter = Encounter::new(new_id, combat_time);
        println!("Loading encounter #{} at timestamp {}", new_id, combat_time);
        encounters.lock().unwrap().insert(new_id, new_encounter);
        *current_encounter = Some(new_id);
        spell_contexts.clear();
        pending_attacks.clear();
        pending_spells.clear();
        long_duration_spells.clear();
    }
    
    // Only update last_combat_time for damage > 0 events (for timeout calculations)
    if should_update_combat_time {
        *last_combat_time = combat_time;
    }
    
    if let Some(encounter_id) = *current_encounter {
        let mut encounters_lock = encounters.lock().unwrap();
        if let Some(encounter) = encounters_lock.get_mut(&encounter_id) {
            encounter.end_time = combat_time;
            
            match parsed {
                ParsedLine::Casting { .. } | ParsedLine::Casts { .. } => {
                    // Ignore casting - only spell resist determines spell damage
                }
                ParsedLine::SpellResist { target, spell, .. } => {
                    // Clear existing pending spells since a new spell resist indicates previous spells didn't result in damage
                    pending_spells.clear();
                    
                    // Check if this is a long-duration spell
                    let is_long_duration = is_long_duration_spell(&spell);
                    
                    if is_long_duration {
                        // For long-duration spells, always create a new tracking entry per target
                        // This ensures each spell resist gets its own tracking regardless of spell type
                        long_duration_spells.push(LongDurationSpell {
                            caster: "Unknown Caster".to_string(), // Will be updated when we see damage
                            target: target.clone(),
                            spell: spell.clone(),
                            timestamp: combat_time,
                            had_save_roll: false,
                            had_damage_immunity: false,
                        });
                        
                        // Also maintain spell context for consistency
                        let mut found = false;
                        for ctx in spell_contexts.iter_mut() {
                            if ctx.spell == spell && !ctx.affected_targets.contains(&target) {
                                ctx.affected_targets.push(target.clone());
                                found = true;
                                break;
                            }
                        }
                        
                        if !found {
                            let new_context = SpellContext {
                                caster: "Unknown Caster".to_string(),
                                spell: spell.clone(),
                                affected_targets: vec![target.clone()],
                                timestamp: combat_time,
                            };
                            spell_contexts.push(new_context);
                        }
                    } else {
                        // For regular spells, use the original logic
                        let mut found = false;
                        for ctx in spell_contexts.iter_mut() {
                            if ctx.spell == spell && !ctx.affected_targets.contains(&target) {
                                ctx.affected_targets.push(target.clone());
                                
                                pending_spells.push(PendingSpell {
                                    caster: ctx.caster.clone(),
                                    target: target.clone(),
                                    spell: spell.clone(),
                                    timestamp: combat_time,
                                    had_save_roll: false,
                                    had_damage_immunity: false,
                                });
                                found = true;
                                break;
                            }
                        }
                        
                        if !found {
                            let new_context = SpellContext {
                                caster: "Unknown Caster".to_string(),
                                spell: spell.clone(),
                                affected_targets: vec![target.clone()],
                                timestamp: combat_time,
                            };
                            
                            pending_spells.push(PendingSpell {
                                caster: "Unknown Caster".to_string(),
                                target: target.clone(),
                                spell: spell.clone(),
                                timestamp: combat_time,
                                had_save_roll: false,
                                had_damage_immunity: false,
                            });
                            
                            spell_contexts.push(new_context);
                        }
                    }
                }
                ParsedLine::Save { target, .. } => {
                    // For saves, match with the most recent spell context and mark pending spells
                    for ctx in spell_contexts.iter_mut() {
                        if ctx.affected_targets.is_empty() || ctx.affected_targets.contains(&target) {
                            if !ctx.affected_targets.contains(&target) {
                                ctx.affected_targets.push(target.clone());
                            }
                            // Mark any pending spells for this target as having had a save roll
                            for pending_spell in pending_spells.iter_mut() {
                                if pending_spell.target == target && pending_spell.spell == ctx.spell {
                                    pending_spell.had_save_roll = true;
                                }
                            }
                            // Mark any long-duration spells for this target as having had a save roll
                            for long_spell in long_duration_spells.iter_mut() {
                                if long_spell.target == target && long_spell.spell == ctx.spell {
                                    long_spell.had_save_roll = true;
                                }
                            }
                            break;
                        }
                    }
                }
                ParsedLine::Attack { attacker, target, result, concealment, timestamp } => {
                    // Clear pending spells when an attack roll happens
                    pending_spells.clear();
                    
                    let attacker_stats = encounter.stats.entry(attacker.clone()).or_default();
                    attacker_stats.update_action_time(timestamp);
                    match result.as_str() {
                        "hit" => {
                            attacker_stats.hits += 1;
                            pending_attacks.push(PendingAttack {
                                attacker: attacker.clone(),
                                target: target.clone(),
                                timestamp: combat_time,
                                is_crit: false,
                            });
                        }
                        "miss" => {
                            attacker_stats.misses += 1;
                            if concealment {
                                attacker_stats.concealment_dodges += 1;
                            }
                        }
                        "critical hit" => {
                            attacker_stats.critical_hits += 1;
                            pending_attacks.push(PendingAttack {
                                attacker: attacker.clone(),
                                target: target.clone(),
                                timestamp: combat_time,
                                is_crit: true,
                            });
                        }
                        _ => {}
                    }
                    encounter.stats.entry(target).or_default().times_attacked += 1;
                }
                ParsedLine::Damage { attacker, target, total, breakdown, timestamp } => {
                    // Clean up expired long-duration spells (older than 6 seconds)
                    long_duration_spells.retain(|spell| {
                        combat_time.saturating_sub(spell.timestamp) <= 6
                    });
                    
                    // STEP 1: Check if this damage matches any active long-duration spells
                    let matching_long_duration_spell = long_duration_spells.iter().find(|spell| {
                        // Check if caster and target match
                        let caster_matches = spell.caster == attacker || spell.caster == "Unknown Caster";
                        let target_matches = spell.target == target;
                        
                        if !caster_matches || !target_matches {
                            return false;
                        }
                        
                        // Check if damage type matches the spell's expected damage type EXCLUSIVELY
                        if let Some(expected_type) = get_spell_damage_type(&spell.spell) {
                            // For specific damage type spells, only match if the damage contains ONLY that type
                            breakdown.len() == 1 && breakdown.contains_key(expected_type)
                        } else {
                            // For unspecified damage types, match any damage
                            true
                        }
                    });
                    
                    // If we found a matching long-duration spell, use it and don't interfere with other tracking
                    let (damage_source, is_from_crit, is_weapon_buff_damage) = if let Some(long_spell) = matching_long_duration_spell {
                        let spell_name = long_spell.spell.clone();
                        let caster_was_unknown = long_spell.caster == "Unknown Caster";
                        
                        // Update spell context caster if it was unknown
                        if caster_was_unknown {
                            for ctx in spell_contexts.iter_mut() {
                                if ctx.spell == spell_name && ctx.caster == "Unknown Caster" {
                                    ctx.caster = attacker.clone();
                                    break;
                                }
                            }
                            
                            // Also update all long-duration spells with unknown caster
                            for long_spell_mut in long_duration_spells.iter_mut() {
                                if long_spell_mut.spell == spell_name && long_spell_mut.caster == "Unknown Caster" {
                                    long_spell_mut.caster = attacker.clone();
                                }
                            }
                        }
                        
                        (format!("Spell: {}", spell_name), false, false)
                    } else {
                        // STEP 2: No long-duration spell matched, use normal attack/spell logic
                        
                        // Find spells with indicators
                        let spell_with_indicators = pending_spells.iter().enumerate().find(|(_, spell)| 
                            (spell.caster == attacker || spell.caster == "Unknown Caster") && 
                            spell.target == target && 
                            (spell.had_save_roll || spell.had_damage_immunity));
                        
                        let oldest_spell = spell_with_indicators.or_else(|| {
                            pending_spells.iter().enumerate().find(|(_, spell)| 
                                (spell.caster == attacker || spell.caster == "Unknown Caster") && spell.target == target)
                        });
                    
                        let oldest_attack = {
                            let mut oldest_idx = None;
                            let mut oldest_timestamp = u64::MAX;
                            
                            for (idx, attack) in pending_attacks.iter().enumerate() {
                                if attack.attacker == attacker && attack.target == target && attack.timestamp < oldest_timestamp {
                                    oldest_idx = Some(idx);
                                    oldest_timestamp = attack.timestamp;
                                }
                            }
                            oldest_idx.map(|idx| (idx, oldest_timestamp))
                        };
                        
                        // Check if this damage is exclusively Fire (weapon buff)
                        let is_weapon_buff = breakdown.len() == 1 && breakdown.contains_key("Fire");
                        
                        if is_weapon_buff && !pending_attacks.is_empty() && pending_spells.is_empty() {
                            // This is weapon buff damage, count as Attack but don't consume the attack
                            ("Attack".to_string(), false, true)
                        } else {
                            match (oldest_spell, oldest_attack) {
                                (Some((spell_idx, spell)), Some((attack_idx, attack_timestamp))) => {
                                    // Both spell and attack found - prioritize spell with save roll or damage immunity, otherwise use timestamp
                                    if spell.had_save_roll || spell.had_damage_immunity || spell.timestamp <= attack_timestamp {
                                        let pending_spell = pending_spells.remove(spell_idx);
                                        
                                        // Update spell context caster if it was unknown
                                        if pending_spell.caster == "Unknown Caster" {
                                            for ctx in spell_contexts.iter_mut() {
                                                if ctx.spell == pending_spell.spell && ctx.caster == "Unknown Caster" {
                                                    ctx.caster = attacker.clone();
                                                    break;
                                                }
                                            }
                                        }
                                        (format!("Spell: {}", pending_spell.spell), false, false)
                                    } else {
                                        let attack = pending_attacks.remove(attack_idx);
                                        ("Attack".to_string(), attack.is_crit, false)
                                    }
                                },
                                (Some((spell_idx, _)), None) => {
                                    // Only spell found
                                    let pending_spell = pending_spells.remove(spell_idx);
                                    
                                    if pending_spell.caster == "Unknown Caster" {
                                        for ctx in spell_contexts.iter_mut() {
                                            if ctx.spell == pending_spell.spell && ctx.caster == "Unknown Caster" {
                                                ctx.caster = attacker.clone();
                                                break;
                                            }
                                        }
                                    }
                                    (format!("Spell: {}", pending_spell.spell), false, false)
                                },
                                (None, Some((attack_idx, _))) => {
                                    // Only attack found
                                    let attack = pending_attacks.remove(attack_idx);
                                    ("Attack".to_string(), attack.is_crit, false)
                                },
                                (None, None) => {
                                    // Neither found
                                    ("Unknown".to_string(), false, false)
                                }
                            }
                        }
                    };
                    
                    // Handle attacker stats
                    {
                        let attacker_stats = encounter.stats.entry(attacker.clone()).or_default();
                        attacker_stats.update_action_time(timestamp);
                        attacker_stats.total_damage_dealt += total;
                        *attacker_stats.damage_by_source_dealt.entry(damage_source.clone()).or_default() += total;
                        
                        // Track damage by target
                        *attacker_stats.damage_by_target_dealt.entry(target.clone()).or_default() += total;
                        *attacker_stats.damage_by_target_and_source_dealt
                            .entry(target.clone())
                            .or_default()
                            .entry(damage_source.clone())
                            .or_default() += total;
                        
                        // Track damage by target, source, and type
                        for (damage_type, &amount) in &breakdown {
                            *attacker_stats.damage_by_target_source_and_type_dealt
                                .entry(target.clone())
                                .or_default()
                                .entry(damage_source.clone())
                                .or_default()
                                .entry(damage_type.clone())
                                .or_default() += amount;
                        }
                        
                        // Track hit vs crit vs weapon buff damage separately for attacks
                        if damage_source == "Attack" {
                            if is_weapon_buff_damage {
                                attacker_stats.weapon_buffs += 1;
                                attacker_stats.weapon_buff_damage += total;
                            } else if is_from_crit {
                                attacker_stats.crit_damage += total;
                            } else {
                                attacker_stats.hit_damage += total;
                            }
                        }
                        
                        for (damage_type, &amount) in &breakdown {
                            *attacker_stats.damage_by_type_dealt.entry(damage_type.clone()).or_default() += amount;
                            
                            // Track hit vs crit vs weapon buff damage by type for attacks
                            if damage_source == "Attack" {
                                if is_weapon_buff_damage {
                                    *attacker_stats.weapon_buff_damage_by_type.entry(damage_type.clone()).or_default() += amount;
                                    *attacker_stats.weapon_buff_damage_by_target_type
                                        .entry(target.clone())
                                        .or_default()
                                        .entry(damage_type.clone())
                                        .or_default() += amount;
                                } else if is_from_crit {
                                    *attacker_stats.crit_damage_by_type.entry(damage_type.clone()).or_default() += amount;
                                    *attacker_stats.crit_damage_by_target_type
                                        .entry(target.clone())
                                        .or_default()
                                        .entry(damage_type.clone())
                                        .or_default() += amount;
                                } else {
                                    *attacker_stats.hit_damage_by_type.entry(damage_type.clone()).or_default() += amount;
                                    *attacker_stats.hit_damage_by_target_type
                                        .entry(target.clone())
                                        .or_default()
                                        .entry(damage_type.clone())
                                        .or_default() += amount;
                                }
                            }
                            
                            // Track damage types per source
                            *attacker_stats.damage_by_source_and_type_dealt
                                .entry(damage_source.clone())
                                .or_default()
                                .entry(damage_type.clone())
                                .or_default() += amount;
                        }
                    }
                    
                    // Handle target stats
                    {
                        let target_stats = encounter.stats.entry(target.clone()).or_default();
                        target_stats.update_action_time(timestamp);
                        target_stats.total_damage_received += total;
                        
                        // Track damage source for received damage
                        let received_source = format!("{} ({})", attacker, damage_source);
                        *target_stats.damage_by_source_received.entry(received_source.clone()).or_default() += total;
                        
                        // Track damage by attacker
                        *target_stats.damage_by_attacker_received.entry(attacker.clone()).or_default() += total;
                        *target_stats.damage_by_attacker_and_source_received
                            .entry(attacker.clone())
                            .or_default()
                            .entry(damage_source.clone())
                            .or_default() += total;
                        
                        for (damage_type, &amount) in &breakdown {
                            *target_stats.damage_by_type_received.entry(damage_type.clone()).or_default() += amount;
                            // Track damage types per source for received damage
                            *target_stats.damage_by_source_and_type_received
                                .entry(received_source.clone())
                                .or_default()
                                .entry(damage_type.clone())
                                .or_default() += amount;
                        }
                    }
                }
                ParsedLine::Absorb { target, amount, dtype, timestamp } => {
                    let target_stats = encounter.stats.entry(target.clone()).or_default();
                    target_stats.update_action_time(timestamp);
                    target_stats.total_damage_absorbed += amount;
                    *target_stats.absorbed_by_type.entry(dtype.clone()).or_default() += amount;
                    
                    // Mark any pending spells for this target as having damage immunity absorption
                    for pending_spell in pending_spells.iter_mut() {
                        if pending_spell.target == target {
                            pending_spell.had_damage_immunity = true;
                        }
                    }
                    
                    // Mark any long-duration spells for this target as having damage immunity absorption
                    for long_spell in long_duration_spells.iter_mut() {
                        if long_spell.target == target {
                            long_spell.had_damage_immunity = true;
                        }
                    }
                }
            }
        }
    }
}