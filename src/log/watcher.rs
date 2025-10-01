use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use regex::Regex;
use lazy_static::lazy_static;
use crate::models::{Encounter, SpellContext, PendingAttack, PendingSpell, LongDurationSpell, PlayerRegistry, BuffTracker, AppSettings};
use crate::parsing::{ParsedLine, parse_log_line, process_parsed_line};
use crate::log::finder::{find_latest_log_file_with_custom_dir, cleanup_old_log_files};
use crate::utils::time::format_duration;
use crate::gui::logs_window::{LogEntry, DamageImmunityAccumulator};

lazy_static! {
    // Regex to match NWN color codes like <c255128000>text</c>
    static ref NWN_COLOR_REGEX: Regex = Regex::new(r"<c(\d{1,3})(\d{1,3})(\d{1,3})>([^<]*)</c>").unwrap();
    // Regex to match damage immunity lines: "Target : Damage Immunity absorbs X point(s) of Type"
    static ref DAMAGE_IMMUNITY_REGEX: Regex = Regex::new(r"^(?P<target>.+?) : Damage Immunity absorbs (?P<amount>\d+) point\(s\) of (?P<type>.+)$").unwrap();
    // Regex to match damage resistance/reduction lines: "Target : Damage Resistance/Reduction absorbs X damage"
    static ref DAMAGE_RESIST_REGEX: Regex = Regex::new(r"^(?P<target>.+?) : Damage (Resistance|Reduction) absorbs (?P<amount>\d+) damage$").unwrap();
    // Regex to match attack lines: "Attacker attacks Target : *hit*"
    static ref ATTACK_REGEX: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) :").unwrap();
}

/// Tracks attack rolls in the logs to distinguish attack immunity from spell immunity
#[derive(Debug, Clone)]
struct PendingAttackInLogs {
    attacker: String,
    target: String,
    timestamp: String,
}

/// Clean NWN color codes from text and remove them
fn clean_nwn_color_codes(text: &str) -> String {
    NWN_COLOR_REGEX.replace_all(text, "$4").to_string()
}

/// Try to parse a damage immunity line
fn parse_damage_immunity(content: &str) -> Option<(String, u32, String)> {
    if let Some(caps) = DAMAGE_IMMUNITY_REGEX.captures(content) {
        let target = caps["target"].trim().to_string();
        let amount = caps["amount"].parse::<u32>().ok()?;
        let dtype = caps["type"].trim().to_string();
        Some((target, amount, dtype))
    } else {
        None
    }
}

/// Try to parse a damage resistance/reduction line
fn parse_damage_resistance(content: &str) -> Option<(String, u32)> {
    if let Some(caps) = DAMAGE_RESIST_REGEX.captures(content) {
        let target = caps["target"].trim().to_string();
        let amount = caps["amount"].parse::<u32>().ok()?;
        Some((target, amount))
    } else {
        None
    }
}

/// Try to parse an attack line
fn parse_attack_line(content: &str) -> Option<(String, String)> {
    if let Some(caps) = ATTACK_REGEX.captures(content) {
        let attacker = caps["attacker"].trim().to_string();
        let target = caps["target"].trim().to_string();
        Some((attacker, target))
    } else {
        None
    }
}

/// Extract damage types from a damage line
/// Example: "damages Target: 49 (39 Physical 3 Acid 2 Divine 5 Pure)" -> ["Physical", "Acid", "Divine", "Pure"]
/// Handles multi-word types like "Negative Energy" and "Positive Energy"
fn extract_damage_types(content: &str) -> Vec<String> {
    let mut damage_types = Vec::new();

    // Find the damage part - everything after "damages" up to ", absorbs:" or ", resisted:" or end
    if let Some(damages_pos) = content.find(" damages ") {
        let after_damages = &content[damages_pos..];

        // Find where the damage breakdown ends (before absorbs/resisted/end)
        let damage_end = after_damages.find(", absorbs:")
            .or_else(|| after_damages.find(", resisted:"))
            .unwrap_or(after_damages.len());

        let damage_part = &after_damages[..damage_end];

        // Find the damage breakdown in parentheses: "N (X Type1 Y Type2 ...)"
        if let Some(open_paren) = damage_part.find('(') {
            if let Some(close_paren) = damage_part[open_paren..].find(')') {
                let breakdown = &damage_part[open_paren + 1..open_paren + close_paren];

                // Parse pairs of "number type"
                let parts: Vec<&str> = breakdown.split_whitespace().collect();
                let mut i = 0;
                while i < parts.len() {
                    // Skip the number, take the type
                    if i + 1 < parts.len() {
                        // Check if current part is a number
                        if parts[i].parse::<u32>().is_ok() {
                            let mut damage_type = parts[i + 1].to_string();

                            // Check if the next word is "Energy" (for multi-word types like "Negative Energy")
                            if i + 2 < parts.len() && parts[i + 2] == "Energy" {
                                damage_type.push_str(" Energy");
                                i += 3; // Skip number, type word, and "Energy"
                            } else {
                                i += 2; // Skip number and type
                            }

                            damage_types.push(damage_type);
                        } else {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
        }
    }

    damage_types
}

/// Takes only the first immunity of each matching damage type from the accumulator.
/// Returns (taken_immunities, remaining_immunities)
fn take_first_matching_immunities(
    absorptions: &Vec<(u32, String)>,
    damage_types: &[String]
) -> (Vec<(u32, String)>, Vec<(u32, String)>) {
    let mut taken = Vec::new();
    let mut remaining = absorptions.clone();

    // For each damage type in the damage line, find and take the first matching immunity
    for dtype in damage_types {
        if let Some(pos) = remaining.iter().position(|(_, itype)| itype == dtype) {
            taken.push(remaining.remove(pos));
        }
    }

    (taken, remaining)
}

pub fn process_full_log_file(
    file_path: &Path,
    encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    current_encounter_id: Arc<Mutex<Option<u64>>>,
    encounter_counter: Arc<Mutex<u64>>,
    player_registry: Arc<Mutex<PlayerRegistry>>,
    buff_tracker: Arc<Mutex<BuffTracker>>,
    settings: &AppSettings,
    logs_state: Arc<Mutex<Vec<LogEntry>>>
) -> io::Result<u64> {
    let file_content = fs::read(file_path)?;

    // Convert bytes to string, replacing invalid UTF-8 sequences
    let content_str = String::from_utf8_lossy(&file_content);

    let mut last_combat_time = 0u64;
    let mut current_encounter: Option<u64> = None;
    let mut spell_contexts: Vec<SpellContext> = Vec::new();
    let mut pending_attacks: Vec<PendingAttack> = Vec::new();
    let mut pending_spells: Vec<PendingSpell> = Vec::new();
    let mut long_duration_spells: Vec<LongDurationSpell> = Vec::new();

    let lines: Vec<&str> = content_str.lines().collect();
    let mut damage_immunity_accumulator: Option<DamageImmunityAccumulator> = None;
    let mut pending_attacks_in_logs: Vec<PendingAttackInLogs> = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        // Process chat window logs for the logs window
        if line.contains("[CHAT WINDOW TEXT]") {
            let timestamp = if let Some(captures) = crate::parsing::regex::RE_TIMESTAMP.captures(line) {
                // Extract just the time portion (HH:MM:SS) from the full timestamp
                let full_timestamp = &captures[1];
                // Format: "Tue Sep 30 14:51:14" - extract "14:51:14"
                if let Some(time_part) = full_timestamp.split(' ').last() {
                    time_part.to_string()
                } else {
                    full_timestamp.to_string()
                }
            } else {
                format!("{:02}:{:02}:{:02}",
                    (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 3600) % 24,
                    (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 60) % 60,
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() % 60)
            };

            // Clean the content: remove [CHAT WINDOW TEXT], timestamp, and color codes
            let cleaned_content = line.trim()
                .strip_prefix("[CHAT WINDOW TEXT]")
                .and_then(|s| s.splitn(2, ']').nth(1))
                .unwrap_or(line)
                .trim();
            let cleaned_content = clean_nwn_color_codes(cleaned_content);

            // Check if this is an attack line and track it
            if let Some((attacker, target)) = parse_attack_line(&cleaned_content) {
                pending_attacks_in_logs.push(PendingAttackInLogs {
                    attacker,
                    target,
                    timestamp: timestamp.clone(),
                });
            }

            // Check ahead for [Talk] tags to update this entry's type
            let mut log_type = crate::gui::logs_window::LogType::from_content(&cleaned_content);
            let mut final_content = cleaned_content.clone();

            // If current line is classified as Other, check next few lines for chat tags
            if log_type == crate::gui::logs_window::LogType::Other {
                // Look ahead up to 3 lines for a chat tag that might refer to this message
                for i in 1..=3 {
                    if let Some(next_line) = lines.get(line_index + i) {
                        if next_line.contains("[Talk]") || next_line.contains("[Tell]") ||
                           next_line.contains("[Party]") || next_line.contains("[Shout]") ||
                           next_line.contains("[Say]") {

                            // Extract the clean content from the next line - handle both formats
                            let next_cleaned = if next_line.contains("[CHAT WINDOW TEXT]") {
                                // Standard format with [CHAT WINDOW TEXT]
                                next_line.trim()
                                    .strip_prefix("[CHAT WINDOW TEXT]")
                                    .and_then(|s| s.splitn(2, ']').nth(1))
                                    .unwrap_or(next_line)
                                    .trim()
                            } else {
                                // Format like "[Zercman] Dank V2: [Talk] talk test"
                                if let Some(bracket_end) = next_line.find(']') {
                                    &next_line[bracket_end + 1..].trim()
                                } else {
                                    next_line.trim()
                                }
                            };
                            let next_cleaned = clean_nwn_color_codes(next_cleaned);

                            // Check if the next line contains our message with a tag
                            if let Some(colon_pos) = next_cleaned.find(": ") {
                                let (next_speaker_part, next_message_part) = next_cleaned.split_at(colon_pos + 2);

                                // Extract the chat tag from the next message
                                let mut found_tag = String::new();
                                for tag in &["[Talk]", "[Tell]", "[Party]", "[Shout]", "[Say]"] {
                                    if next_message_part.contains(tag) {
                                        found_tag = tag.to_string();
                                        break;
                                    }
                                }

                                // Remove tags from the next message to compare
                                let next_message_without_tags = next_message_part
                                    .replace("[Talk] ", "")
                                    .replace("[Tell] ", "")
                                    .replace("[Party] ", "")
                                    .replace("[Shout] ", "")
                                    .replace("[Say] ", "");

                                // Check if our current message matches the tagless version
                                if let Some(current_colon_pos) = cleaned_content.find(": ") {
                                    let (current_speaker_part, current_message_part) = cleaned_content.split_at(current_colon_pos + 2);

                                    if current_speaker_part == next_speaker_part &&
                                       current_message_part.trim() == next_message_without_tags.trim() {
                                        // Update this line's type to Chat and modify content to include the tag
                                        log_type = crate::gui::logs_window::LogType::Chat;
                                        final_content = format!("{}{} {}", current_speaker_part, found_tag, current_message_part.trim());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Check if this is a damage immunity line
            if let Some((target, amount, dtype)) = parse_damage_immunity(&final_content) {
                // Check if there are recent attacks for this target (indicates attack immunity, not spell)
                let has_recent_attacks = pending_attacks_in_logs.iter().any(|atk|
                    atk.target == target && atk.timestamp == timestamp
                );

                // Check if accumulator already exists for this target/timestamp
                if let Some(ref mut acc) = damage_immunity_accumulator {
                    if acc.timestamp == timestamp && acc.target == target {
                        // Add to existing accumulator
                        acc.absorptions.push((amount, dtype));
                    } else {
                        // Different context - flush old accumulator first
                        if let Ok(mut logs) = logs_state.lock() {
                            for entry in logs.iter_mut().rev().take(10) {
                                if entry.timestamp == acc.timestamp &&
                                   entry.content.contains(&format!("damages {}", &acc.target)) {
                                    // Update this entry with accumulated data
                                    if let Some(absorbs_pos) = entry.content.find(", absorbs:") {
                                        entry.content = entry.content[..absorbs_pos].to_string();
                                    } else if let Some(resisted_pos) = entry.content.find(", resisted:") {
                                        entry.content = entry.content[..resisted_pos].to_string();
                                    }
                                    let absorption_suffix = acc.format_absorption_suffix();
                                    entry.content += &absorption_suffix;
                                    break;
                                }
                            }
                        }
                        // Start new accumulator
                        damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                            timestamp: timestamp.clone(),
                            target: target.clone(),
                            absorptions: vec![(amount, dtype)],
                            resistance_total: 0,
                            is_attack_immunity: has_recent_attacks,
                        });
                    }
                } else {
                    // No accumulator, start one
                    damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                        timestamp: timestamp.clone(),
                        target: target.clone(),
                        absorptions: vec![(amount, dtype)],
                        resistance_total: 0,
                        is_attack_immunity: has_recent_attacks,
                    });
                }
                // Don't skip - let this line be parsed for stats tracking
            }

            if let Some((target, amount)) = parse_damage_resistance(&final_content) {
                // This is a damage resistance/reduction line
                // Check if there are recent attacks for this target
                let has_recent_attacks = pending_attacks_in_logs.iter().any(|atk|
                    atk.target == target && atk.timestamp == timestamp
                );

                // Add to accumulator
                if let Some(ref mut acc) = damage_immunity_accumulator {
                    if acc.timestamp == timestamp && acc.target == target {
                        acc.resistance_total += amount;
                    } else {
                        // Different context - flush old accumulator first
                        if let Ok(mut logs) = logs_state.lock() {
                            for entry in logs.iter_mut().rev().take(10) {
                                if entry.timestamp == acc.timestamp &&
                                   entry.content.contains(&format!("damages {}", &acc.target)) {
                                    // Update this entry with accumulated data
                                    if let Some(absorbs_pos) = entry.content.find(", absorbs:") {
                                        entry.content = entry.content[..absorbs_pos].to_string();
                                    } else if let Some(resisted_pos) = entry.content.find(", resisted:") {
                                        entry.content = entry.content[..resisted_pos].to_string();
                                    }
                                    let absorption_suffix = acc.format_absorption_suffix();
                                    entry.content += &absorption_suffix;
                                    break;
                                }
                            }
                        }
                        // Start new accumulator with this resistance
                        damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                            timestamp: timestamp.clone(),
                            target: target.clone(),
                            absorptions: Vec::new(),
                            resistance_total: amount,
                            is_attack_immunity: has_recent_attacks,
                        });
                    }
                } else {
                    // No accumulator, start one
                    damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                        timestamp: timestamp.clone(),
                        target: target.clone(),
                        absorptions: Vec::new(),
                        resistance_total: amount,
                        is_attack_immunity: has_recent_attacks,
                    });
                }
                // Don't skip - let this line be parsed for stats tracking
            }

            // Process other lines (damage, attacks, etc.) that are not immunity/resistance
            if !final_content.contains("Damage Immunity") && !final_content.contains("Damage Resistance") && !final_content.contains("Damage Reduction") {
                // This is NOT an absorption or resistance line
                // Check if this is a damage line
                let is_damage_line = final_content.contains("damages") && final_content.contains(":");

                if is_damage_line {
                    // This is a damage line - check if we have pending immunities
                    if let Some(ref acc) = damage_immunity_accumulator {
                        // Extract target from damage line
                        if let Some(damages_pos) = final_content.find(" damages ") {
                            let after_damages = &final_content[damages_pos + 9..];
                            if let Some(colon_pos) = after_damages.find(":") {
                                let damage_target = after_damages[..colon_pos].trim();

                                // Check if this matches our accumulator
                                if acc.target == damage_target && acc.timestamp == timestamp {
                                    // This is a SECOND damage line for the same target/timestamp
                                    // Flush accumulator to the PREVIOUS damage line, keeping remaining for this line
                                    let mut remaining_absorptions = acc.absorptions.clone();

                                    if let Ok(mut logs) = logs_state.lock() {
                                        for entry in logs.iter_mut().rev().take(10) {
                                            if entry.timestamp == acc.timestamp &&
                                               entry.content.contains(&format!("damages {}", &acc.target)) &&
                                               !entry.content.contains(", absorbs:") &&
                                               !entry.content.contains(", resisted:") {
                                                // Extract damage types from the previous damage line
                                                let damage_types = extract_damage_types(&entry.content);

                                                // Take first matching immunity of each type
                                                let (taken_absorptions, leftover) =
                                                    take_first_matching_immunities(&remaining_absorptions, &damage_types);

                                                if !taken_absorptions.is_empty() || acc.resistance_total > 0 {
                                                    // Apply to previous damage line
                                                    let temp_acc = DamageImmunityAccumulator {
                                                        timestamp: acc.timestamp.clone(),
                                                        target: acc.target.clone(),
                                                        absorptions: taken_absorptions,
                                                        resistance_total: acc.resistance_total,
                                                        is_attack_immunity: acc.is_attack_immunity,
                                                    };
                                                    let absorption_suffix = temp_acc.format_absorption_suffix();
                                                    entry.content += &absorption_suffix;
                                                }

                                                // Update remaining for current line
                                                remaining_absorptions = leftover;
                                                break;
                                            }
                                        }
                                    }

                                    // Now apply remaining immunities to THIS line
                                    let current_damage_types = extract_damage_types(&final_content);
                                    let (current_taken, _) =
                                        take_first_matching_immunities(&remaining_absorptions, &current_damage_types);

                                    if !current_taken.is_empty() {
                                        let temp_acc = DamageImmunityAccumulator {
                                            timestamp: timestamp.clone(),
                                            target: acc.target.clone(),
                                            absorptions: current_taken,
                                            resistance_total: 0, // Resistance only applied to first line
                                            is_attack_immunity: acc.is_attack_immunity,
                                        };
                                        let absorption_suffix = temp_acc.format_absorption_suffix();
                                        final_content = final_content + &absorption_suffix;
                                    }

                                    // Clear accumulator - all immunities have been distributed
                                    damage_immunity_accumulator = None;
                                }
                            }
                        }
                    }
                } else {
                    // NOT a damage line - try to flush any pending accumulator to PREVIOUS damage line (AFTER-damage case)
                    if let Some(ref acc) = damage_immunity_accumulator {
                        let mut remaining_absorptions = acc.absorptions.clone();

                        if let Ok(mut logs) = logs_state.lock() {
                            for entry in logs.iter_mut().rev().take(10) {
                                if entry.timestamp == acc.timestamp &&
                                   entry.content.contains(&format!("damages {}", &acc.target)) {
                                    // Skip lines that already have absorbs or resisted (from BEFORE-damage case)
                                    if entry.content.contains(", absorbs:") || entry.content.contains(", resisted:") {
                                        continue;
                                    }

                                    // Extract damage types and take only first of each matching type
                                    let damage_types = extract_damage_types(&entry.content);
                                    let (taken_absorptions, leftover_absorptions) =
                                        take_first_matching_immunities(&remaining_absorptions, &damage_types);

                                    if !taken_absorptions.is_empty() || acc.resistance_total > 0 {
                                        // Update this entry with taken immunities
                                        let temp_acc = DamageImmunityAccumulator {
                                            timestamp: acc.timestamp.clone(),
                                            target: acc.target.clone(),
                                            absorptions: taken_absorptions,
                                            resistance_total: acc.resistance_total,
                                            is_attack_immunity: acc.is_attack_immunity,
                                        };
                                        let absorption_suffix = temp_acc.format_absorption_suffix();
                                        entry.content += &absorption_suffix;

                                        // Update remaining for potential next damage line
                                        remaining_absorptions = leftover_absorptions;

                                        // If no more immunities remain, we're done
                                        if remaining_absorptions.is_empty() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        // Update accumulator with remaining immunities, or clear if empty
                        if remaining_absorptions.is_empty() {
                            damage_immunity_accumulator = None;
                        } else {
                            damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                                timestamp: acc.timestamp.clone(),
                                target: acc.target.clone(),
                                absorptions: remaining_absorptions,
                                resistance_total: 0, // Resistance is only applied once
                                is_attack_immunity: acc.is_attack_immunity,
                            });
                        }
                    }
                }
            }

            // Only add to logs window if this is NOT an immunity/resistance line
            // (those are accumulated and added as suffixes to damage lines)
            let is_immunity_line = final_content.contains("Damage Immunity absorbs")
                || final_content.contains("Damage Resistance absorbs")
                || final_content.contains("Damage Reduction absorbs");

            if !is_immunity_line {
                let log_entry = LogEntry {
                    timestamp: timestamp.clone(),
                    content: final_content.clone(),
                    log_type: log_type.clone(),
                };

                if let Ok(mut logs) = logs_state.lock() {
                    logs.push(log_entry);
                }
            }
        }

        if let Some(parsed) = parse_log_line(&line) {
            // Check if this is a buff expiration that should be ignored (recast scenario)
            if let ParsedLine::BuffExpired { spell_name, timestamp } = &parsed {
                // Check previous line to see if it's a recast of the same buff
                if line_index > 0 {
                    if let Some(prev_parsed) = parse_log_line(lines[line_index - 1]) {
                        if let ParsedLine::Casts { spell, timestamp: prev_timestamp, .. } = prev_parsed {
                            // If previous line is casting the same spell with same timestamp, skip this "wore off"
                            if spell == *spell_name && prev_timestamp == *timestamp {
                                continue; // Skip processing this buff expiration
                            }
                        }
                    }
                }
            }

            let combat_time = match &parsed {
                ParsedLine::Attack { timestamp, .. } => *timestamp,
                ParsedLine::Damage { timestamp, .. } => *timestamp,
                ParsedLine::Absorb { timestamp, .. } => *timestamp,
                ParsedLine::AbsorbResistance { timestamp, .. } => *timestamp,
                ParsedLine::AbsorbReduction { timestamp, .. } => *timestamp,
                ParsedLine::SpellResist { timestamp, .. } => *timestamp,
                ParsedLine::Save { timestamp, .. } => *timestamp,
                ParsedLine::Casting { timestamp, .. } => *timestamp,
                ParsedLine::Casts { timestamp, .. } => *timestamp,
                ParsedLine::PlayerJoin { timestamp, .. } => *timestamp,
                ParsedLine::PlayerChat { timestamp, .. } => *timestamp,
                ParsedLine::PartyChat { timestamp, .. } => *timestamp,
                ParsedLine::PartyJoin { timestamp, .. } => *timestamp,
                ParsedLine::Resting { timestamp, .. } => *timestamp,
                ParsedLine::BuffExpired { timestamp, .. } => *timestamp,
            };

            process_parsed_line(
                parsed,
                combat_time,
                &mut last_combat_time,
                &mut current_encounter,
                &mut spell_contexts,
                &mut pending_attacks,
                &mut pending_spells,
                &mut long_duration_spells,
                &encounters,
                &encounter_counter,
                &player_registry,
                &buff_tracker,
                settings,
                true  // is_historical = true for initial log processing
            );
        }
    }

    // Flush any remaining damage immunity accumulator by updating the damage line
    if let Some(acc) = damage_immunity_accumulator.take() {
        println!("=== END OF FILE FLUSH ===");
        println!("Accumulator - Target: '{}', Timestamp: '{}'", acc.target, acc.timestamp);
        println!("Accumulator - Absorptions: {:?}", acc.absorptions);
        println!("Accumulator - Resistance Total: {}", acc.resistance_total);

        if let Ok(mut logs) = logs_state.lock() {
            println!("Total log entries: {}", logs.len());
            let search_str = format!("damages {}", &acc.target);
            println!("Searching for: '{}'", search_str);

            // Search backwards to find the matching damage line
            let mut found = false;
            for (idx, entry) in logs.iter_mut().rev().take(10).enumerate() {
                println!("Entry {}: timestamp='{}', content starts with='{}'",
                    idx, entry.timestamp, &entry.content[..entry.content.len().min(50)]);

                // More robust search: check timestamp, "damages" keyword, and target name separately
                let has_damages = entry.content.contains("damages");
                let has_target = entry.content.contains(&acc.target);
                let timestamp_match = entry.timestamp == acc.timestamp;

                if timestamp_match && has_damages && has_target {
                    println!("FOUND MATCHING DAMAGE LINE!");
                    println!("Before update: {}", entry.content);

                    // Extract damage types and filter matching absorptions
                    let damage_types = extract_damage_types(&entry.content);
                    println!("Damage types: {:?}", damage_types);

                    let matching_absorptions: Vec<(u32, String)> = acc.absorptions.iter()
                        .filter(|(_, dtype)| damage_types.iter().any(|dt| dt == dtype))
                        .cloned()
                        .collect();
                    println!("Matching absorptions: {:?}", matching_absorptions);

                    if !matching_absorptions.is_empty() || acc.resistance_total > 0 {
                        // Update this entry with accumulated data
                        if let Some(absorbs_pos) = entry.content.find(", absorbs:") {
                            entry.content = entry.content[..absorbs_pos].to_string();
                        } else if let Some(resisted_pos) = entry.content.find(", resisted:") {
                            entry.content = entry.content[..resisted_pos].to_string();
                        }
                        let temp_acc = DamageImmunityAccumulator {
                            timestamp: acc.timestamp.clone(),
                            target: acc.target.clone(),
                            absorptions: matching_absorptions,
                            resistance_total: acc.resistance_total,
                            is_attack_immunity: acc.is_attack_immunity,
                        };
                        let absorption_suffix = temp_acc.format_absorption_suffix();
                        entry.content += &absorption_suffix;
                        println!("After update: {}", entry.content);
                        found = true;
                    }
                    break;
                }
            }
            if !found {
                println!("ERROR: No matching damage line found!");
            }
        }
        println!("=== END FLUSH ===\n");
    }

    // Set the current encounter to the most recent one
    *current_encounter_id.lock().unwrap() = current_encounter;
    
    // Update most damaged participant for all encounters
    {
        let mut encounters_lock = encounters.lock().unwrap();
        for encounter in encounters_lock.values_mut() {
            encounter.update_most_damaged();
            println!("Encounter #{}: {} ({})", encounter.id, encounter.get_display_name(), format_duration(encounter.duration()));
        }
    }
    
    let file_size = fs::metadata(file_path)?.len();
    Ok(file_size)
}

pub fn log_watcher_thread(
    encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    current_encounter_id: Arc<Mutex<Option<u64>>>,
    encounter_counter: Arc<Mutex<u64>>,
    player_registry: Arc<Mutex<PlayerRegistry>>,
    buff_tracker: Arc<Mutex<BuffTracker>>,
    settings: Arc<Mutex<AppSettings>>,
    log_reload_requested: Arc<Mutex<bool>>,
    logs_state: Arc<Mutex<Vec<LogEntry>>>
) {
    let mut last_read_position = 0u64;
    let mut current_log_path: Option<PathBuf> = None;
    let mut last_combat_time = 0u64;
    let mut current_encounter: Option<u64> = None;
    let mut spell_contexts: Vec<SpellContext> = Vec::new();
    let mut pending_attacks: Vec<PendingAttack> = Vec::new();
    let mut pending_spells: Vec<PendingSpell> = Vec::new();
    let mut long_duration_spells: Vec<LongDurationSpell> = Vec::new();
    let mut damage_immunity_accumulator: Option<DamageImmunityAccumulator> = None;
    let mut pending_attacks_in_logs: Vec<PendingAttackInLogs> = Vec::new();

    // Perform cleanup of old log files at startup
    match cleanup_old_log_files() {
        Ok(count) => {
            if count > 0 {
                println!("Cleaned up {} old log files", count);
            }
        }
        Err(e) => println!("Error during log cleanup: {}", e),
    }

    let mut cleanup_counter = 0;
    const CLEANUP_INTERVAL: u32 = 6000; // Clean up every 10 minutes (6000 * 100ms)

    loop {
        // Check if log reload was requested
        if let Ok(mut reload_flag) = log_reload_requested.lock() {
            if *reload_flag {
                println!("Log reload requested - clearing data and forcing re-detection of log files");

                // Clear existing data immediately
                encounters.lock().unwrap().clear();
                *current_encounter_id.lock().unwrap() = None;
                *encounter_counter.lock().unwrap() = 1;
                spell_contexts.clear();
                pending_attacks.clear();
                pending_spells.clear();
                long_duration_spells.clear();
                damage_immunity_accumulator = None; // Reset accumulator
                pending_attacks_in_logs.clear();

                current_log_path = None; // Force re-detection
                last_read_position = 0; // Reset file position
                *reload_flag = false; // Reset the flag
            }
        }

        // Get the custom log directory from settings if available
        let custom_log_dir = if let Ok(settings_guard) = settings.lock() {
            settings_guard.log_directory.clone()
        } else {
            None
        };

        if let Some(latest_log_path) = find_latest_log_file_with_custom_dir(custom_log_dir.as_deref()) {
            if current_log_path.as_ref() != Some(&latest_log_path) {
                println!("\n--- Detected new log file: {:?} ---\n", latest_log_path);
                current_log_path = Some(latest_log_path.clone());

                // Clear existing data when switching to a different log file
                encounters.lock().unwrap().clear();
                *current_encounter_id.lock().unwrap() = None;
                *encounter_counter.lock().unwrap() = 1;
                spell_contexts.clear();
                pending_attacks.clear();
                pending_spells.clear();
                long_duration_spells.clear();
                damage_immunity_accumulator = None; // Reset accumulator
                pending_attacks_in_logs.clear();

                // Process the entire log file to set up historical encounters
                println!("Processing entire log file for historical data...");
                // Get current settings for processing
                let current_settings = if let Ok(settings_guard) = settings.lock() {
                    settings_guard.clone()
                } else {
                    AppSettings::default()
                };

                match process_full_log_file(&latest_log_path, encounters.clone(), current_encounter_id.clone(), encounter_counter.clone(), player_registry.clone(), buff_tracker.clone(), &current_settings, logs_state.clone()) {
                    Ok(file_size) => {
                        last_read_position = file_size;
                        let encounter_count = encounters.lock().unwrap().len();
                        println!("Loaded {} historical encounters from log file", encounter_count);
                    }
                    Err(e) => {
                        println!("Error processing log file: {}", e);
                        last_read_position = 0;
                    }
                }
                
                // Get the last combat time from the most recent encounter and sync current_encounter
                if let Some(most_recent) = encounters.lock().unwrap().values().max_by_key(|e| e.end_time) {
                    last_combat_time = most_recent.end_time;
                }
                current_encounter = *current_encounter_id.lock().unwrap();
            }

            // Continue monitoring for new log entries
            if let Some(path_to_read) = &current_log_path {
                if let Ok(metadata) = fs::metadata(path_to_read) {
                    let current_size = metadata.len();
                    if current_size > last_read_position {
                        if let Ok(file) = fs::File::open(path_to_read) {
                            let mut reader = BufReader::new(file);
                            if reader.seek(SeekFrom::Start(last_read_position)).is_ok() {
                                // Read remaining bytes and convert to string
                                let mut buffer = Vec::new();
                                if reader.read_to_end(&mut buffer).is_ok() {
                                    let content_str = String::from_utf8_lossy(&buffer);
                                    let new_lines: Vec<&str> = content_str.lines().collect();

                                    for (line_index, line) in new_lines.iter().enumerate() {
                                        // Add log entry for the logs window - only process [CHAT WINDOW TEXT] lines
                                        if line.contains("[CHAT WINDOW TEXT]") {
                                            let timestamp = if let Some(captures) = crate::parsing::regex::RE_TIMESTAMP.captures(line) {
                                                // Extract just the time portion (HH:MM:SS) from the full timestamp
                                                let full_timestamp = &captures[1];
                                                // Format: "Tue Sep 30 14:51:14" - extract "14:51:14"
                                                if let Some(time_part) = full_timestamp.split(' ').last() {
                                                    time_part.to_string()
                                                } else {
                                                    full_timestamp.to_string()
                                                }
                                            } else {
                                                format!("{:02}:{:02}:{:02}",
                                                    (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 3600) % 24,
                                                    (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 60) % 60,
                                                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() % 60)
                                            };

                                            // Clean the content: remove [CHAT WINDOW TEXT], timestamp, and color codes
                                            let cleaned_content = line.trim()
                                                .strip_prefix("[CHAT WINDOW TEXT]")
                                                .and_then(|s| s.splitn(2, ']').nth(1))
                                                .unwrap_or(line)
                                                .trim();
                                            let cleaned_content = clean_nwn_color_codes(cleaned_content);

                                            // Check if this is an attack line and track it
                                            if let Some((attacker, target)) = parse_attack_line(&cleaned_content) {
                                                pending_attacks_in_logs.push(PendingAttackInLogs {
                                                    attacker,
                                                    target,
                                                    timestamp: timestamp.clone(),
                                                });
                                            }

                                            // Check ahead for [Talk] tags to update this entry's type
                                            let mut log_type = crate::gui::logs_window::LogType::from_content(&cleaned_content);
                                            let mut final_content = cleaned_content.clone();

                                            // If current line is classified as Other, check next few lines for chat tags
                                            if log_type == crate::gui::logs_window::LogType::Other {
                                                // Look ahead up to 3 lines for a chat tag that might refer to this message
                                                for i in 1..=3 {
                                                    if let Some(next_line) = new_lines.get(line_index + i) {
                                                        if next_line.contains("[Talk]") || next_line.contains("[Tell]") ||
                                                           next_line.contains("[Party]") || next_line.contains("[Shout]") ||
                                                           next_line.contains("[Say]") {

                                                            // Extract the clean content from the next line - handle both formats
                                                            let next_cleaned = if next_line.contains("[CHAT WINDOW TEXT]") {
                                                                // Standard format with [CHAT WINDOW TEXT]
                                                                next_line.trim()
                                                                    .strip_prefix("[CHAT WINDOW TEXT]")
                                                                    .and_then(|s| s.splitn(2, ']').nth(1))
                                                                    .unwrap_or(next_line)
                                                                    .trim()
                                                            } else {
                                                                // Format like "[Zercman] Dank V2: [Talk] talk test"
                                                                if let Some(bracket_end) = next_line.find(']') {
                                                                    &next_line[bracket_end + 1..].trim()
                                                                } else {
                                                                    next_line.trim()
                                                                }
                                                            };
                                                            let next_cleaned = clean_nwn_color_codes(next_cleaned);

                                                            // Check if the next line contains our message with a tag
                                                            if let Some(colon_pos) = next_cleaned.find(": ") {
                                                                let (next_speaker_part, next_message_part) = next_cleaned.split_at(colon_pos + 2);

                                                                // Extract the chat tag from the next message
                                                                let mut found_tag = String::new();
                                                                for tag in &["[Talk]", "[Tell]", "[Party]", "[Shout]", "[Say]"] {
                                                                    if next_message_part.contains(tag) {
                                                                        found_tag = tag.to_string();
                                                                        break;
                                                                    }
                                                                }

                                                                // Remove tags from the next message to compare
                                                                let next_message_without_tags = next_message_part
                                                                    .replace("[Talk] ", "")
                                                                    .replace("[Tell] ", "")
                                                                    .replace("[Party] ", "")
                                                                    .replace("[Shout] ", "")
                                                                    .replace("[Say] ", "");

                                                                // Check if our current message matches the tagless version
                                                                if let Some(current_colon_pos) = cleaned_content.find(": ") {
                                                                    let (current_speaker_part, current_message_part) = cleaned_content.split_at(current_colon_pos + 2);

                                                                    if current_speaker_part == next_speaker_part &&
                                                                       current_message_part.trim() == next_message_without_tags.trim() {
                                                                        // Update this line's type to Chat and modify content to include the tag
                                                                        log_type = crate::gui::logs_window::LogType::Chat;
                                                                        final_content = format!("{}{} {}", current_speaker_part, found_tag, current_message_part.trim());
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // Check if this is a damage immunity line
                                            if let Some((target, amount, dtype)) = parse_damage_immunity(&final_content) {
                                                // Check if there are recent attacks for this target (indicates attack immunity, not spell)
                                                let has_recent_attacks = pending_attacks_in_logs.iter().any(|atk|
                                                    atk.target == target && atk.timestamp == timestamp
                                                );

                                                // Check if accumulator already exists for this target/timestamp
                                                if let Some(ref mut acc) = damage_immunity_accumulator {
                                                    if acc.timestamp == timestamp && acc.target == target {
                                                        // Add to existing accumulator - just accumulate, don't update yet
                                                        acc.absorptions.push((amount, dtype));
                                                    } else {
                                                        // Different context - flush old accumulator first
                                                        if let Ok(mut logs) = logs_state.lock() {
                                                            for entry in logs.iter_mut().rev().take(10) {
                                                                if entry.timestamp == acc.timestamp &&
                                                                   entry.content.contains(&format!("damages {}", &acc.target)) {
                                                                    // Extract damage types and filter matching absorptions
                                                                    let damage_types = extract_damage_types(&entry.content);
                                                                    let matching_absorptions: Vec<(u32, String)> = acc.absorptions.iter()
                                                                        .filter(|(_, dtype)| damage_types.iter().any(|dt| dt == dtype))
                                                                        .cloned()
                                                                        .collect();

                                                                    if !matching_absorptions.is_empty() {
                                                                        // Update this entry with ALL accumulated matching data
                                                                        if let Some(absorbs_pos) = entry.content.find(", absorbs:") {
                                                                            entry.content = entry.content[..absorbs_pos].to_string();
                                                                        } else if let Some(resisted_pos) = entry.content.find(", resisted:") {
                                                                            entry.content = entry.content[..resisted_pos].to_string();
                                                                        }
                                                                        let temp_acc = DamageImmunityAccumulator {
                                                                            timestamp: acc.timestamp.clone(),
                                                                            target: acc.target.clone(),
                                                                            absorptions: matching_absorptions,
                                                                            resistance_total: acc.resistance_total,
                                                                            is_attack_immunity: acc.is_attack_immunity,
                                                                        };
                                                                        let absorption_suffix = temp_acc.format_absorption_suffix();
                                                                        entry.content += &absorption_suffix;
                                                                    }
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        // Start new accumulator
                                                        damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                                                            timestamp: timestamp.clone(),
                                                            target: target.clone(),
                                                            absorptions: vec![(amount, dtype)],
                                                            resistance_total: 0,
                                                            is_attack_immunity: has_recent_attacks,
                                                        });
                                                    }
                                                } else {
                                                    // No accumulator, start one
                                                    damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                                                        timestamp: timestamp.clone(),
                                                        target: target.clone(),
                                                        absorptions: vec![(amount, dtype)],
                                                        resistance_total: 0,
                                                        is_attack_immunity: has_recent_attacks,
                                                    });
                                                }
                                                continue; // Skip adding this line as a separate entry
                                            } else if let Some((target, amount)) = parse_damage_resistance(&final_content) {
                                                // This is a damage resistance/reduction line
                                                // Check if there are recent attacks for this target
                                                let has_recent_attacks = pending_attacks_in_logs.iter().any(|atk|
                                                    atk.target == target && atk.timestamp == timestamp
                                                );

                                                if let Some(ref mut acc) = damage_immunity_accumulator {
                                                    if acc.timestamp == timestamp && acc.target == target {
                                                        acc.resistance_total += amount;
                                                    } else {
                                                        // Different context - flush old accumulator first
                                                        if let Ok(mut logs) = logs_state.lock() {
                                                            for entry in logs.iter_mut().rev().take(10) {
                                                                if entry.timestamp == acc.timestamp &&
                                                                   entry.content.contains(&format!("damages {}", &acc.target)) {
                                                                    // Extract damage types and filter matching absorptions
                                                                    let damage_types = extract_damage_types(&entry.content);
                                                                    let matching_absorptions: Vec<(u32, String)> = acc.absorptions.iter()
                                                                        .filter(|(_, dtype)| damage_types.iter().any(|dt| dt == dtype))
                                                                        .cloned()
                                                                        .collect();

                                                                    if !matching_absorptions.is_empty() || acc.resistance_total > 0 {
                                                                        // Update this entry with accumulated data
                                                                        if let Some(absorbs_pos) = entry.content.find(", absorbs:") {
                                                                            entry.content = entry.content[..absorbs_pos].to_string();
                                                                        } else if let Some(resisted_pos) = entry.content.find(", resisted:") {
                                                                            entry.content = entry.content[..resisted_pos].to_string();
                                                                        }
                                                                        let temp_acc = DamageImmunityAccumulator {
                                                                            timestamp: acc.timestamp.clone(),
                                                                            target: acc.target.clone(),
                                                                            absorptions: matching_absorptions,
                                                                            resistance_total: acc.resistance_total,
                                                                            is_attack_immunity: acc.is_attack_immunity,
                                                                        };
                                                                        let absorption_suffix = temp_acc.format_absorption_suffix();
                                                                        entry.content += &absorption_suffix;
                                                                    }
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        // Start new accumulator with this resistance
                                                        damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                                                            timestamp: timestamp.clone(),
                                                            target: target.clone(),
                                                            absorptions: Vec::new(),
                                                            resistance_total: amount,
                                                            is_attack_immunity: has_recent_attacks,
                                                        });
                                                    }
                                                } else {
                                                    // No accumulator, start one
                                                    damage_immunity_accumulator = Some(DamageImmunityAccumulator {
                                                        timestamp: timestamp.clone(),
                                                        target: target.clone(),
                                                        absorptions: Vec::new(),
                                                        resistance_total: amount,
                                                        is_attack_immunity: has_recent_attacks,
                                                    });
                                                }
                                                continue;
                                            } else {
                                                // This is NOT an absorption or resistance line
                                                // Flush any pending accumulator if it exists
                                                if let Some(ref acc) = damage_immunity_accumulator {
                                                    // If this is a damage line for the same target/timestamp
                                                    if final_content.contains("damages") && final_content.contains(":") {
                                                        // Extract target from damage line
                                                        if let Some(damages_pos) = final_content.find(" damages ") {
                                                            let after_damages = &final_content[damages_pos + 9..];
                                                            if let Some(colon_pos) = after_damages.find(":") {
                                                                let damage_target = after_damages[..colon_pos].trim();

                                                                // Check if this matches our accumulator
                                                                if acc.target == damage_target && acc.timestamp == timestamp {
                                                                    // This is a SECOND damage line for the same target/timestamp
                                                                    // Flush accumulator to the PREVIOUS damage line, keeping remaining for this line
                                                                    let mut remaining_absorptions = acc.absorptions.clone();

                                                                    if let Ok(mut logs) = logs_state.lock() {
                                                                        for entry in logs.iter_mut().rev().take(10) {
                                                                            if entry.timestamp == acc.timestamp &&
                                                                               entry.content.contains(&format!("damages {}", &acc.target)) &&
                                                                               !entry.content.contains(", absorbs:") &&
                                                                               !entry.content.contains(", resisted:") {
                                                                                // Extract damage types from the previous damage line
                                                                                let damage_types = extract_damage_types(&entry.content);

                                                                                // Take first matching immunity of each type
                                                                                let (taken_absorptions, leftover) =
                                                                                    take_first_matching_immunities(&remaining_absorptions, &damage_types);

                                                                                if !taken_absorptions.is_empty() || acc.resistance_total > 0 {
                                                                                    // Apply to previous damage line
                                                                                    let temp_acc = DamageImmunityAccumulator {
                                                                                        timestamp: acc.timestamp.clone(),
                                                                                        target: acc.target.clone(),
                                                                                        absorptions: taken_absorptions,
                                                                                        resistance_total: acc.resistance_total,
                                                                                        is_attack_immunity: acc.is_attack_immunity,
                                                                                    };
                                                                                    let absorption_suffix = temp_acc.format_absorption_suffix();
                                                                                    entry.content += &absorption_suffix;
                                                                                }

                                                                                // Update remaining for current line
                                                                                remaining_absorptions = leftover;
                                                                                break;
                                                                            }
                                                                        }
                                                                    }

                                                                    // Now apply remaining immunities to THIS line
                                                                    let current_damage_types = extract_damage_types(&final_content);
                                                                    let (current_taken, _) =
                                                                        take_first_matching_immunities(&remaining_absorptions, &current_damage_types);

                                                                    if !current_taken.is_empty() {
                                                                        let temp_acc = DamageImmunityAccumulator {
                                                                            timestamp: timestamp.clone(),
                                                                            target: acc.target.clone(),
                                                                            absorptions: current_taken,
                                                                            resistance_total: 0, // Resistance only applied to first line
                                                                            is_attack_immunity: acc.is_attack_immunity,
                                                                        };
                                                                        let absorption_suffix = temp_acc.format_absorption_suffix();
                                                                        final_content = final_content + &absorption_suffix;
                                                                    }

                                                                    // Clear accumulator - all immunities have been distributed
                                                                    damage_immunity_accumulator = None;
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        // Not a damage line - flush the accumulator if we have one
                                                        // This handles both different timestamp AND same timestamp but different content
                                                        if let Ok(mut logs) = logs_state.lock() {
                                                            for entry in logs.iter_mut().rev().take(10) {
                                                                if entry.timestamp == acc.timestamp &&
                                                                   entry.content.contains(&format!("damages {}", &acc.target)) {
                                                                    // Extract damage types and filter matching absorptions
                                                                    let damage_types = extract_damage_types(&entry.content);
                                                                    let matching_absorptions: Vec<(u32, String)> = acc.absorptions.iter()
                                                                        .filter(|(_, dtype)| damage_types.iter().any(|dt| dt == dtype))
                                                                        .cloned()
                                                                        .collect();

                                                                    if !matching_absorptions.is_empty() || acc.resistance_total > 0 {
                                                                        // Update this entry with accumulated data
                                                                        if let Some(absorbs_pos) = entry.content.find(", absorbs:") {
                                                                            entry.content = entry.content[..absorbs_pos].to_string();
                                                                        } else if let Some(resisted_pos) = entry.content.find(", resisted:") {
                                                                            entry.content = entry.content[..resisted_pos].to_string();
                                                                        }
                                                                        let temp_acc = DamageImmunityAccumulator {
                                                                            timestamp: acc.timestamp.clone(),
                                                                            target: acc.target.clone(),
                                                                            absorptions: matching_absorptions,
                                                                            resistance_total: acc.resistance_total,
                                                                            is_attack_immunity: acc.is_attack_immunity,
                                                                        };
                                                                        let absorption_suffix = temp_acc.format_absorption_suffix();
                                                                        entry.content += &absorption_suffix;
                                                                    }
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        damage_immunity_accumulator = None;
                                                    }
                                                }
                                            }

                                            let log_entry = LogEntry {
                                                timestamp: timestamp.clone(),
                                                content: final_content.clone(),
                                                log_type: log_type.clone(),
                                            };

                                            if let Ok(mut logs) = logs_state.lock() {
                                                let mut should_add = true;
                                                let mut found_match_to_update = false;

                                                // Check for duplicates and handle chat tag updates
                                                for entry in logs.iter_mut().rev().take(5) {
                                                    // Check for exact match
                                                    if entry.content == cleaned_content {
                                                        should_add = false;
                                                        break;
                                                    }

                                                    // Check for chat message that needs tag update
                                                    if let Some(colon_pos) = cleaned_content.find(": ") {
                                                        let (speaker_part, message_part) = cleaned_content.split_at(colon_pos + 2);

                                                        // If the current message has a chat tag and we can match it to a previous tagless message
                                                        if message_part.contains("[Talk]") || message_part.contains("[Tell]") ||
                                                           message_part.contains("[Party]") || message_part.contains("[Shout]") ||
                                                           message_part.contains("[Say]") {

                                                            // Remove chat type tags to find the base message
                                                            let message_without_tags = message_part
                                                                .replace("[Talk] ", "")
                                                                .replace("[Tell] ", "")
                                                                .replace("[Party] ", "")
                                                                .replace("[Shout] ", "")
                                                                .replace("[Say] ", "");

                                                            let simplified_content = format!("{}{}", speaker_part, message_without_tags);

                                                            // If we find a previous entry that matches (without tag), update it
                                                            if entry.content == simplified_content && entry.log_type == crate::gui::logs_window::LogType::Other {
                                                                entry.content = cleaned_content.clone();
                                                                entry.log_type = log_type.clone();
                                                                should_add = false;
                                                                found_match_to_update = true;
                                                                break;
                                                            }
                                                        }
                                                        // If current message has no tag, check if we match a tagged version
                                                        else {
                                                            let base_content = format!("{}{}", speaker_part, message_part);
                                                            // Check if entry has the tagged version of this message
                                                            if entry.content.starts_with(&speaker_part) {
                                                                let entry_message_part = &entry.content[speaker_part.len()..];
                                                                let entry_without_tags = entry_message_part
                                                                    .replace("[Talk] ", "")
                                                                    .replace("[Tell] ", "")
                                                                    .replace("[Party] ", "")
                                                                    .replace("[Shout] ", "")
                                                                    .replace("[Say] ", "");

                                                                if message_part.trim() == entry_without_tags.trim() {
                                                                    should_add = false;
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                if should_add {
                                                    logs.push(log_entry);
                                                }
                                            }
                                        }

                                        if let Some(parsed) = parse_log_line(&line) {
                                        // Check if this is a buff expiration that should be ignored (recast scenario)
                                        if let ParsedLine::BuffExpired { spell_name, timestamp } = &parsed {
                                            // Check previous line to see if it's a recast of the same buff
                                            if line_index > 0 {
                                                if let Some(prev_parsed) = parse_log_line(new_lines[line_index - 1]) {
                                                    if let ParsedLine::Casts { spell, timestamp: prev_timestamp, .. } = prev_parsed {
                                                        // If previous line is casting the same spell with same timestamp, skip this "wore off"
                                                        if spell == *spell_name && prev_timestamp == *timestamp {
                                                            continue; // Skip processing this buff expiration
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        let combat_time = match &parsed {
                                            ParsedLine::Attack { timestamp, .. } => *timestamp,
                                            ParsedLine::Damage { timestamp, .. } => *timestamp,
                                            ParsedLine::Absorb { timestamp, .. } => *timestamp,
                                            ParsedLine::AbsorbResistance { timestamp, .. } => *timestamp,
                                            ParsedLine::AbsorbReduction { timestamp, .. } => *timestamp,
                                            ParsedLine::SpellResist { timestamp, .. } => *timestamp,
                                            ParsedLine::Save { timestamp, .. } => *timestamp,
                                            ParsedLine::Casting { timestamp, .. } => *timestamp,
                                            ParsedLine::Casts { timestamp, .. } => *timestamp,
                                            ParsedLine::PlayerJoin { timestamp, .. } => *timestamp,
                                            ParsedLine::PlayerChat { timestamp, .. } => *timestamp,
                                            ParsedLine::PartyChat { timestamp, .. } => *timestamp,
                                            ParsedLine::PartyJoin { timestamp, .. } => *timestamp,
                                            ParsedLine::Resting { timestamp, .. } => *timestamp,
                                            ParsedLine::BuffExpired { timestamp, .. } => *timestamp,
                                        };

                                        // Get current settings for processing
                                        let current_settings = if let Ok(settings_guard) = settings.lock() {
                                            settings_guard.clone()
                                        } else {
                                            AppSettings::default()
                                        };

                                        // Use the centralized processing function
                                        process_parsed_line(
                                            parsed,
                                            combat_time,
                                            &mut last_combat_time,
                                            &mut current_encounter,
                                            &mut spell_contexts,
                                            &mut pending_attacks,
                                            &mut pending_spells,
                                            &mut long_duration_spells,
                                            &encounters,
                                            &encounter_counter,
                                            &player_registry,
                                            &buff_tracker,
                                            &current_settings,
                                            false  // is_historical = false for real-time processing
                                        );

                                        // Update the shared current_encounter_id when it changes
                                        if let Ok(mut shared_current) = current_encounter_id.lock() {
                                            if *shared_current != current_encounter {
                                                *shared_current = current_encounter;
                                                if let Some(encounter_id) = current_encounter {
                                                    println!("Started new encounter #{} at timestamp {}", encounter_id, combat_time);
                                                }
                                            }
                                        }

                                        // Update most damaged for the current encounter
                                        if let Some(encounter_id) = current_encounter {
                                            if let Ok(mut encounters_lock) = encounters.lock() {
                                                if let Some(encounter) = encounters_lock.get_mut(&encounter_id) {
                                                    encounter.update_most_damaged();
                                                }
                                            }
                                        }
                                    }
                                }
                                }
                            }
                        }
                        last_read_position = current_size;
                    }
                }
            }
        } else {
            // No log files found in the specified directory
            if current_log_path.is_some() {
                println!("No log files found in directory - clearing current log path");
                current_log_path = None;
                last_read_position = 0;
            }
        }

        // Periodic cleanup of old log files
        cleanup_counter += 1;
        if cleanup_counter >= CLEANUP_INTERVAL {
            cleanup_counter = 0;
            match cleanup_old_log_files() {
                Ok(count) => {
                    if count > 0 {
                        println!("Periodic cleanup: removed {} old log files", count);
                    }
                }
                Err(e) => println!("Error during periodic cleanup: {}", e),
            }
        }
        
        thread::sleep(Duration::from_millis(100));
    }
}