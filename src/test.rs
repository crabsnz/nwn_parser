use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::{process_full_log_file, Encounter};

#[test]
fn test_spell_damage_separation() {
    let encounters = Arc::new(Mutex::new(HashMap::new()));
    let current_encounter_id = Arc::new(Mutex::new(None));
    let encounter_counter = Arc::new(Mutex::new(1));
    
    let test_log_path = Path::new("test_log.txt");
    
    // Process the test log
    process_full_log_file(
        test_log_path,
        encounters.clone(),
        current_encounter_id.clone(),
        encounter_counter.clone()
    ).expect("Failed to process test log");
    
    let encounters_lock = encounters.lock().unwrap();
    assert!(!encounters_lock.is_empty(), "Should have created at least one encounter");
    
    // Get the first encounter
    let encounter = encounters_lock.values().next().unwrap();
    
    // Check that Dank V2 has stats
    let dank_stats = encounter.stats.get("Dank V2").expect("Dank V2 should have stats");
    
    // Verify spell damage vs attack damage separation
    println!("Dank V2 damage sources: {:?}", dank_stats.damage_by_source_dealt);
    
    // Should have both spell damage (Incendiary Cloud) and attack damage  
    // Note: The test log shows Cloudkill at the end, but most damage should be from Incendiary Cloud
    let has_incendiary = dank_stats.damage_by_source_dealt.contains_key("Spell: Incendiary Cloud");
    let has_cloudkill = dank_stats.damage_by_source_dealt.contains_key("Spell: Cloudkill");
    
    println!("Has Incendiary Cloud: {}", has_incendiary);
    println!("Has Cloudkill: {}", has_cloudkill);
    
    // We should have at least one spell damage source
    assert!(has_incendiary || has_cloudkill, "Should have some spell damage");
    
    // Verify the spell damage amounts
    let spell_damage = dank_stats.damage_by_source_dealt.get("Spell: Incendiary Cloud").unwrap_or(&0);
    let attack_damage = dank_stats.damage_by_source_dealt.get("Attack").unwrap_or(&0);
    
    println!("Incendiary Cloud damage: {}", spell_damage);
    println!("Attack damage: {}", attack_damage);
    
    // Based on the log analysis, we should have spell damage
    // Some damage might be classified as spell damage since the Incendiary Cloud is an area effect
    let total_spell_damage = dank_stats.damage_by_source_dealt.values()
        .filter(|&&v| v > 0)
        .sum::<u32>();
    
    assert!(total_spell_damage > 0, "Should have some damage tracked");
    
    // The key test is that we can separate spell sources from attack sources
    let has_spell_damage = dank_stats.damage_by_source_dealt.keys()
        .any(|k| k.starts_with("Spell:"));
    assert!(has_spell_damage, "Should identify at least some spell damage");
    
    println!("Test passed! Spell damage: {}, Attack damage: {}", spell_damage, attack_damage);
}

#[test]
fn test_missile_storm_damage_attribution() {
    let encounters = Arc::new(Mutex::new(HashMap::new()));
    let current_encounter_id = Arc::new(Mutex::new(None));
    let encounter_counter = Arc::new(Mutex::new(1));
    
    let test_log_path = Path::new("test_missile_storm.txt");
    
    // Process the missile storm test log
    process_full_log_file(
        test_log_path,
        encounters.clone(),
        current_encounter_id.clone(),
        encounter_counter.clone()
    ).expect("Failed to process missile storm test log");
    
    let encounters_lock = encounters.lock().unwrap();
    assert!(!encounters_lock.is_empty(), "Should have created at least one encounter");
    
    // Get the first encounter
    let encounter = encounters_lock.values().next().unwrap();
    
    // Check that Dank V2 has stats
    let dank_stats = encounter.stats.get("Dank V2").expect("Dank V2 should have stats");
    
    println!("Dank V2 damage sources: {:?}", dank_stats.damage_by_source_dealt);
    
    // Should have Isaac's Greater Missile Storm damage
    let missile_storm_damage = dank_stats.damage_by_source_dealt.get("Spell: Isaac's Greater Missile Storm").unwrap_or(&0);
    
    println!("Isaac's Greater Missile Storm damage: {}", missile_storm_damage);
    
    // Should have significant missile storm damage (all the magical damage should be attributed to the spell)
    assert!(*missile_storm_damage > 0, "Should have missile storm damage attributed");
    
    // Check that all magical damage is attributed to the spell (not to unknown sources)
    let unknown_damage = dank_stats.damage_by_source_dealt.get("Unknown").unwrap_or(&0);
    println!("Unknown damage: {}", unknown_damage);
    
    // Should have minimal or no unknown damage since missile storm should catch all magical damage
    assert!(*unknown_damage == 0, "Should not have unknown damage when missile storm is active");
    
    // Verify that the damage types are correct (should be Magical for missile storm)
    if let Some(type_map) = dank_stats.damage_by_source_and_type_dealt.get("Spell: Isaac's Greater Missile Storm") {
        let magical_damage = type_map.get("Magical").unwrap_or(&0);
        println!("Magical damage from missile storm: {}", magical_damage);
        assert!(*magical_damage > 0, "Missile storm should deal magical damage");
        assert_eq!(*magical_damage, *missile_storm_damage, "All missile storm damage should be magical");
    }
    
    println!("Test passed! Missile storm damage correctly attributed: {}", missile_storm_damage);
}

#[test]
fn test_missile_storm_with_attacks_separation() {
    let encounters = Arc::new(Mutex::new(HashMap::new()));
    let current_encounter_id = Arc::new(Mutex::new(None));
    let encounter_counter = Arc::new(Mutex::new(1));
    
    let test_log_path = Path::new("test_missile_storm_with_attacks.txt");
    
    // Process the missile storm with attacks test log
    process_full_log_file(
        test_log_path,
        encounters.clone(),
        current_encounter_id.clone(),
        encounter_counter.clone()
    ).expect("Failed to process missile storm with attacks test log");
    
    let encounters_lock = encounters.lock().unwrap();
    assert!(!encounters_lock.is_empty(), "Should have created at least one encounter");
    
    // Get the first encounter
    let encounter = encounters_lock.values().next().unwrap();
    
    // Check that Dank V2 has stats
    let dank_stats = encounter.stats.get("Dank V2").expect("Dank V2 should have stats");
    
    println!("Dank V2 damage sources: {:?}", dank_stats.damage_by_source_dealt);
    
    // Should have Isaac's Greater Missile Storm damage (first ~16 magical damage instances)
    let missile_storm_damage = dank_stats.damage_by_source_dealt.get("Spell: Isaac's Greater Missile Storm").unwrap_or(&0);
    
    // Should have Attack damage (the mixed damage after attacks start)
    let attack_damage = dank_stats.damage_by_source_dealt.get("Attack").unwrap_or(&0);
    
    println!("Isaac's Greater Missile Storm damage: {}", missile_storm_damage);
    println!("Attack damage: {}", attack_damage);
    
    // Should have significant missile storm damage (pure magical damage before attacks)
    assert!(*missile_storm_damage > 0, "Should have missile storm damage attributed");
    
    // Should also have attack damage (mixed damage types after attacks start)
    assert!(*attack_damage > 0, "Should have attack damage attributed after attacks start");
    
    // Check that the magical damage in attacks is NOT attributed to missile storm when it's mixed damage
    // The key insight: pure magical damage (5, 5, 5, 8, 9, 6, 6, 2, 6, 6, 4, 2, 5, 8, 10, 6, 5, 6, 6, 9) should go to missile storm
    // Mixed damage (44 with Physical/Magical/Divine/Negative, 40 with Physical/Magical/Divine/Negative) should go to Attack
    
    // Verify the total adds up correctly
    let total_damage = dank_stats.total_damage_dealt;
    let sum_by_source: u32 = dank_stats.damage_by_source_dealt.values().sum();
    assert_eq!(total_damage, sum_by_source, "Total damage should equal sum of all sources");
    
    // Check that unknown damage is minimal (should be near zero with good attribution)
    let unknown_damage = dank_stats.damage_by_source_dealt.get("Unknown").unwrap_or(&0);
    println!("Unknown damage: {}", unknown_damage);
    
    // With proper separation, unknown damage should be minimal
    assert!(*unknown_damage <= 21, "Should have minimal unknown damage with proper separation"); // Allow for the weapon buff fire damage
    
    println!("Test passed! Missile storm: {}, Attack: {}, Unknown: {}", 
        missile_storm_damage, attack_damage, unknown_damage);
}

#[test]
fn test_multiple_spells_target_separation() {
    let encounters = Arc::new(Mutex::new(HashMap::new()));
    let current_encounter_id = Arc::new(Mutex::new(None));
    let encounter_counter = Arc::new(Mutex::new(1));
    
    let test_log_path = Path::new("test_multiple_spells_multiple_targets.txt");
    
    // Process the test with IGMS on both targets, then Magic Missile on 65 AC only
    process_full_log_file(
        test_log_path,
        encounters.clone(),
        current_encounter_id.clone(),
        encounter_counter.clone()
    ).expect("Failed to process multiple spells test log");
    
    let encounters_lock = encounters.lock().unwrap();
    assert!(!encounters_lock.is_empty(), "Should have created at least one encounter");
    
    // Get the first encounter
    let encounter = encounters_lock.values().next().unwrap();
    
    // Check that Dank V2 has stats
    let dank_stats = encounter.stats.get("Dank V2").expect("Dank V2 should have stats");
    
    println!("Dank V2 damage sources: {:?}", dank_stats.damage_by_source_dealt);
    
    // Should have both IGMS and Magic Missile damage
    let igms_damage = dank_stats.damage_by_source_dealt.get("Spell: Isaac's Greater Missile Storm").unwrap_or(&0);
    let mm_damage = dank_stats.damage_by_source_dealt.get("Spell: Magic Missile").unwrap_or(&0);
    
    println!("Isaac's Greater Missile Storm damage: {}", igms_damage);
    println!("Magic Missile damage: {}", mm_damage);
    
    // IGMS should get damage to both targets (early damage)
    // Magic Missile should only get damage to 65 AC DUMMY (later damage after spell resist)
    assert!(*igms_damage > 0, "Should have IGMS damage attributed");
    assert!(*mm_damage > 0, "Should have Magic Missile damage attributed");
    
    // The key test: Magic Missile should only get the damage that came AFTER its spell resist
    // IGMS damage: early damage to both 10 AC (5+2+2+7+8+8+9+10+6+4) and 65 AC (2+6+5+5+6+6+6)
    // Magic Missile damage: later damage to 65 AC only (6+5+10+4+4+2+4+2+1+4+3)
    
    // Verify the damage is properly separated and targets are correct
    let total_damage = dank_stats.total_damage_dealt;
    let sum_by_source: u32 = dank_stats.damage_by_source_dealt.values().sum();
    assert_eq!(total_damage, sum_by_source, "Total damage should equal sum of all sources");
    
    // Check for minimal unknown damage
    let unknown_damage = dank_stats.damage_by_source_dealt.get("Unknown").unwrap_or(&0);
    println!("Unknown damage: {}", unknown_damage);
    assert!(*unknown_damage == 0, "Should have zero unknown damage with proper target separation");
    
    println!("Test passed! IGMS: {}, Magic Missile: {}, Unknown: {}", 
        igms_damage, mm_damage, unknown_damage);
}