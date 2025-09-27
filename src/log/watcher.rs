use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crate::models::{Encounter, SpellContext, PendingAttack, PendingSpell, LongDurationSpell, PlayerRegistry, BuffTracker, AppSettings};
use crate::parsing::{ParsedLine, parse_log_line, process_parsed_line};
use crate::log::finder::{find_latest_log_file, cleanup_old_log_files};
use crate::utils::time::format_duration;

pub fn process_full_log_file(
    file_path: &Path,
    encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    current_encounter_id: Arc<Mutex<Option<u64>>>,
    encounter_counter: Arc<Mutex<u64>>,
    player_registry: Arc<Mutex<PlayerRegistry>>,
    buff_tracker: Arc<Mutex<BuffTracker>>,
    settings: &AppSettings
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
    
    for line in content_str.lines() {
        if let Some(parsed) = parse_log_line(&line) {
            let combat_time = match &parsed {
                ParsedLine::Attack { timestamp, .. } => *timestamp,
                ParsedLine::Damage { timestamp, .. } => *timestamp,
                ParsedLine::Absorb { timestamp, .. } => *timestamp,
                ParsedLine::SpellResist { timestamp, .. } => *timestamp,
                ParsedLine::Save { timestamp, .. } => *timestamp,
                ParsedLine::Casting { timestamp, .. } => *timestamp,
                ParsedLine::Casts { timestamp, .. } => *timestamp,
                ParsedLine::PlayerJoin { timestamp, .. } => *timestamp,
                ParsedLine::PlayerChat { timestamp, .. } => *timestamp,
                ParsedLine::PartyChat { timestamp, .. } => *timestamp,
                ParsedLine::PartyJoin { timestamp, .. } => *timestamp,
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
    settings: Arc<Mutex<AppSettings>>
) {
    let mut last_read_position = 0u64;
    let mut current_log_path: Option<PathBuf> = None;
    let mut last_combat_time = 0u64;
    let mut current_encounter: Option<u64> = None;
    let mut spell_contexts: Vec<SpellContext> = Vec::new();
    let mut pending_attacks: Vec<PendingAttack> = Vec::new();
    let mut pending_spells: Vec<PendingSpell> = Vec::new();
    let mut long_duration_spells: Vec<LongDurationSpell> = Vec::new();

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
        if let Some(latest_log_path) = find_latest_log_file() {
            if current_log_path.as_ref() != Some(&latest_log_path) {
                println!("\n--- Detected new log file: {:?} ---\n", latest_log_path);
                current_log_path = Some(latest_log_path.clone());
                
                // Clear existing data
                encounters.lock().unwrap().clear();
                *current_encounter_id.lock().unwrap() = None;
                *encounter_counter.lock().unwrap() = 1;
                spell_contexts.clear();
                pending_attacks.clear();
                pending_spells.clear();
                long_duration_spells.clear();
                
                // Process the entire log file to set up historical encounters
                println!("Processing entire log file for historical data...");
                // Get current settings for processing
                let current_settings = if let Ok(settings_guard) = settings.lock() {
                    settings_guard.clone()
                } else {
                    AppSettings::new()
                };

                match process_full_log_file(&latest_log_path, encounters.clone(), current_encounter_id.clone(), encounter_counter.clone(), player_registry.clone(), buff_tracker.clone(), &current_settings) {
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
                                    for line in content_str.lines() {
                                        if let Some(parsed) = parse_log_line(&line) {
                                        let combat_time = match &parsed {
                                            ParsedLine::Attack { timestamp, .. } => *timestamp,
                                            ParsedLine::Damage { timestamp, .. } => *timestamp,
                                            ParsedLine::Absorb { timestamp, .. } => *timestamp,
                                            ParsedLine::SpellResist { timestamp, .. } => *timestamp,
                                            ParsedLine::Save { timestamp, .. } => *timestamp,
                                            ParsedLine::Casting { timestamp, .. } => *timestamp,
                                            ParsedLine::Casts { timestamp, .. } => *timestamp,
                                            ParsedLine::PlayerJoin { timestamp, .. } => *timestamp,
                                            ParsedLine::PlayerChat { timestamp, .. } => *timestamp,
                                            ParsedLine::PartyChat { timestamp, .. } => *timestamp,
                                            ParsedLine::PartyJoin { timestamp, .. } => *timestamp,
                                        };
                                        
                                        // Get current settings for processing
                                        let current_settings = if let Ok(settings_guard) = settings.lock() {
                                            settings_guard.clone()
                                        } else {
                                            AppSettings::new()
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