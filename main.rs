// main.rs

#[cfg(test)]
mod test;

use eframe::{egui, NativeOptions};
use egui::ViewportBuilder;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// --- Helper Functions ---

fn format_duration(seconds: u64) -> String {
    if seconds >= 60 {
        let minutes = seconds / 60;
        let remaining_seconds = seconds % 60;
        format!("[{}m:{}s]", minutes, remaining_seconds)
    } else {
        format!("[{}s]", seconds)
    }
}

// --- Data Structures to hold parsed information ---

#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    CurrentFight,
    OverallStats,
    MultipleSelected,
}

/// Holds the aggregated statistics for a single combatant.
#[derive(Debug, Default, Clone)]
struct CombatantStats {
    // --- Stats for actions performed by the combatant ---
    hits: u32,
    misses: u32,
    critical_hits: u32,
    concealment_dodges: u32,
    weapon_buffs: u32,
    total_damage_dealt: u32,
    hit_damage: u32,
    crit_damage: u32,
    weapon_buff_damage: u32,
    damage_by_type_dealt: HashMap<String, u32>,
    hit_damage_by_type: HashMap<String, u32>,
    crit_damage_by_type: HashMap<String, u32>,
    weapon_buff_damage_by_type: HashMap<String, u32>,
    damage_by_source_dealt: HashMap<String, u32>, // "Attack", "Spell: Fireball", etc.
    damage_by_source_and_type_dealt: HashMap<String, HashMap<String, u32>>, // Source -> Type -> Amount
    damage_by_target_dealt: HashMap<String, u32>, // Target -> Total damage to that target
    damage_by_target_and_source_dealt: HashMap<String, HashMap<String, u32>>, // Target -> Source -> Amount
    damage_by_target_source_and_type_dealt: HashMap<String, HashMap<String, HashMap<String, u32>>>, // Target -> Source -> Type -> Amount
    hit_damage_by_target_type: HashMap<String, HashMap<String, u32>>, // Target -> Type -> Amount (for hit damage only)
    crit_damage_by_target_type: HashMap<String, HashMap<String, u32>>, // Target -> Type -> Amount (for crit damage only)
    weapon_buff_damage_by_target_type: HashMap<String, HashMap<String, u32>>, // Target -> Type -> Amount (for weapon buff damage only)

    // --- Stats for actions received by the combatant ---
    times_attacked: u32,
    total_damage_received: u32,
    damage_by_type_received: HashMap<String, u32>,
    damage_by_source_received: HashMap<String, u32>, // Track who/what damaged this combatant
    damage_by_source_and_type_received: HashMap<String, HashMap<String, u32>>, // Source -> Type -> Amount
    damage_by_attacker_received: HashMap<String, u32>, // Attacker -> Total damage from that attacker
    damage_by_attacker_and_source_received: HashMap<String, HashMap<String, u32>>, // Attacker -> Source -> Amount

    // --- Special stats like absorption ---
    total_damage_absorbed: u32,
    absorbed_by_type: HashMap<String, u32>,
    
    // --- Timing for DPS calculation ---
    first_action_time: Option<u64>,
    last_action_time: Option<u64>,
}

impl CombatantStats {
    fn calculate_dps(&self) -> Option<f64> {
        if let (Some(first), Some(last)) = (self.first_action_time, self.last_action_time) {
            let duration_secs = if last > first { last - first } else { 1 };
            if duration_secs > 0 && self.total_damage_dealt > 0 {
                Some(self.total_damage_dealt as f64 / duration_secs as f64)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    fn calculate_source_dps(&self, damage_amount: u32) -> Option<f64> {
        // Use the same time window as total damage dealt DPS
        if let (Some(first), Some(last)) = (self.first_action_time, self.last_action_time) {
            let duration_secs = if last > first { last - first } else { 1 };
            if duration_secs > 0 && damage_amount > 0 {
                Some(damage_amount as f64 / duration_secs as f64)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    fn update_action_time(&mut self, timestamp: u64) {
        if self.first_action_time.is_none() {
            self.first_action_time = Some(timestamp);
        }
        self.last_action_time = Some(timestamp);
    }
    
}

/// Represents a single encounter/fight
#[derive(Debug, Clone)]
struct Encounter {
    id: u64,
    start_time: u64,
    end_time: u64,
    stats: HashMap<String, CombatantStats>,
    most_damaged_participant: String,
    total_damage: u32,
}

impl Encounter {
    fn new(id: u64, start_time: u64) -> Self {
        Self {
            id,
            start_time,
            end_time: start_time,
            stats: HashMap::new(),
            most_damaged_participant: String::new(),
            total_damage: 0,
        }
    }

    fn update_most_damaged(&mut self) {
        let mut max_damage = 0;
        let mut most_damaged = String::new();
        
        // First, try to find the participant who received the most damage
        for (name, stats) in &self.stats {
            if stats.total_damage_received > max_damage {
                max_damage = stats.total_damage_received;
                most_damaged = name.clone();
            }
        }
        
        // If no one has taken damage yet, find the most attacked participant
        if most_damaged.is_empty() {
            let mut max_attacks = 0;
            for (name, stats) in &self.stats {
                if stats.times_attacked > max_attacks {
                    max_attacks = stats.times_attacked;
                    most_damaged = name.clone();
                }
            }
        }
        
        self.most_damaged_participant = most_damaged;
        self.total_damage = self.stats.values().map(|s| s.total_damage_dealt).sum();
    }

    fn duration(&self) -> u64 {
        if self.end_time >= self.start_time {
            self.end_time - self.start_time
        } else {
            0
        }
    }

    fn get_display_name(&self) -> String {
        let duration_str = format_duration(self.duration());
        if self.most_damaged_participant.is_empty() {
            format!("#{} {} Fight", self.id, duration_str)
        } else {
            format!("#{} {} {}", self.id, duration_str, self.most_damaged_participant)
        }
    }
}

// --- GUI Application State ---

/// This struct holds the state of our GUI application.
struct NwnLogApp {
    /// All encounters, indexed by encounter ID
    encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    /// The current encounter being tracked
    current_encounter_id: Arc<Mutex<Option<u64>>>,
    /// Selected encounters for display (supports multiple selection)
    selected_encounter_ids: std::collections::HashSet<u64>,
    /// Current view mode: individual encounters or combined view
    view_mode: ViewMode,
    /// Track if we're in resize mode
    is_resizing: bool,
    /// Minimum window size
    min_size: egui::Vec2,
    /// Text scaling factor
    text_scale: f32,
    /// Encounter counter
    encounter_counter: Arc<Mutex<u64>>,
    /// Cached sorted combatants to avoid re-sorting every frame
    cached_sorted_combatants: Vec<(String, CombatantStats)>,
    /// Hash of the current data to detect changes
    last_data_hash: u64,
}

impl NwnLogApp {
    fn new() -> Self {
        Self {
            encounters: Arc::new(Mutex::new(HashMap::new())),
            current_encounter_id: Arc::new(Mutex::new(None)),
            selected_encounter_ids: HashSet::new(),
            view_mode: ViewMode::CurrentFight,
            is_resizing: false,
            min_size: egui::Vec2::new(300.0, 250.0),
            text_scale: 1.0,
            encounter_counter: Arc::new(Mutex::new(1)),
            cached_sorted_combatants: Vec::new(),
            last_data_hash: 0,
        }
    }

    /// Helper function to create a custom collapsible header with full click area
    fn custom_collapsing_header(
        &self, 
        ui: &mut egui::Ui, 
        id: egui::Id, 
        text: &str,
        content: impl FnOnce(&mut egui::Ui)
    ) {
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        
        // Calculate header height and width
        let header_height = 20.0;
        let available_width = ui.available_width();
        let header_rect = ui.allocate_space(egui::Vec2::new(available_width, header_height)).1;
        
        // Create clickable header
        let header_response = ui.allocate_rect(header_rect, egui::Sense::click());
        
        // Draw header content (non-interactive)
        let text_painter = ui.painter();
        
        // Draw collapsing arrow
        let arrow_text = if state.is_open() { "▼" } else { "▶" };
        let arrow_pos = egui::Pos2::new(header_rect.min.x + 8.0, header_rect.center().y);
        text_painter.text(arrow_pos, egui::Align2::LEFT_CENTER, arrow_text, egui::FontId::default(), ui.visuals().text_color());
        
        // Draw header text
        let text_pos = egui::Pos2::new(header_rect.min.x + 25.0, header_rect.center().y);
        text_painter.text(text_pos, egui::Align2::LEFT_CENTER, text, 
            egui::FontId::proportional(13.0), ui.visuals().text_color());
        
        // Handle click
        if header_response.clicked() {
            state.toggle(ui);
        }
        
        state.store(ui.ctx());
        
        // Show content if expanded
        if state.is_open() {
            ui.indent(id, |ui| {
                content(ui);
            });
        }
    }

    fn get_current_stats(&self) -> HashMap<String, CombatantStats> {
        // If encounters are selected, always show combined encounter stats
        if !self.selected_encounter_ids.is_empty() {
            return self.get_combined_selected_stats_safe();
        }
        
        match self.view_mode {
            ViewMode::CurrentFight => {
                // Check if there's an active fight
                if let Ok(current_id) = self.current_encounter_id.try_lock() {
                    if let Some(current_id) = *current_id {
                        if let Ok(encounters) = self.encounters.try_lock() {
                            if let Some(encounter) = encounters.get(&current_id) {
                                // Check if the fight is still active (within 5 seconds)
                                let current_time = get_current_timestamp();
                                if current_time >= encounter.end_time && current_time - encounter.end_time <= 5 {
                                    return encounter.stats.clone();
                                }
                            }
                        }
                    }
                }
                
                // No active fight, return empty stats
                HashMap::new()
            },
            ViewMode::OverallStats => {
                self.get_overall_stats_safe()
            },
            ViewMode::MultipleSelected => {
                self.get_combined_selected_stats_safe()
            }
        }
    }

    fn get_combined_selected_stats_safe(&self) -> HashMap<String, CombatantStats> {
        if let Ok(encounters) = self.encounters.try_lock() {
            self.combine_selected_encounters_stats(&encounters)
        } else {
            HashMap::new()
        }
    }

    fn get_combined_selected_stats(&self) -> HashMap<String, CombatantStats> {
        let encounters = self.encounters.lock().unwrap();
        self.combine_selected_encounters_stats(&encounters)
    }

    fn combine_selected_encounters_stats(&self, encounters: &HashMap<u64, Encounter>) -> HashMap<String, CombatantStats> {
        let mut combined_stats = HashMap::new();
        
        // Combine stats from all selected encounters
        for encounter_id in &self.selected_encounter_ids {
            if let Some(encounter) = encounters.get(encounter_id) {
                for (name, stats) in &encounter.stats {
                    let combined = combined_stats.entry(name.clone()).or_insert_with(CombatantStats::default);
                    
                    // Aggregate all stats (same logic as get_overall_stats)
                    combined.hits += stats.hits;
                    combined.misses += stats.misses;
                    combined.critical_hits += stats.critical_hits;
                    combined.times_attacked += stats.times_attacked;
                    combined.total_damage_dealt += stats.total_damage_dealt;
                    combined.hit_damage += stats.hit_damage;
                    combined.crit_damage += stats.crit_damage;
                    combined.weapon_buff_damage += stats.weapon_buff_damage;
                    combined.total_damage_received += stats.total_damage_received;
                    combined.total_damage_absorbed += stats.total_damage_absorbed;
                    
                    // Aggregate damage by type dealt
                    for (dtype, amount) in &stats.damage_by_type_dealt {
                        *combined.damage_by_type_dealt.entry(dtype.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate hit damage by type
                    for (dtype, amount) in &stats.hit_damage_by_type {
                        *combined.hit_damage_by_type.entry(dtype.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate crit damage by type
                    for (dtype, amount) in &stats.crit_damage_by_type {
                        *combined.crit_damage_by_type.entry(dtype.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate weapon buff damage by type
                    for (dtype, amount) in &stats.weapon_buff_damage_by_type {
                        *combined.weapon_buff_damage_by_type.entry(dtype.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate damage sources
                    for (source, amount) in &stats.damage_by_source_dealt {
                        *combined.damage_by_source_dealt.entry(source.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate damage by source and type dealt
                    for (source, type_map) in &stats.damage_by_source_and_type_dealt {
                        let combined_type_map = combined.damage_by_source_and_type_dealt.entry(source.clone()).or_default();
                        for (dtype, amount) in type_map {
                            *combined_type_map.entry(dtype.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate damage by target
                    for (target, amount) in &stats.damage_by_target_dealt {
                        *combined.damage_by_target_dealt.entry(target.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate damage by target and source dealt
                    for (target, source_map) in &stats.damage_by_target_and_source_dealt {
                        let combined_target_map = combined.damage_by_target_and_source_dealt.entry(target.clone()).or_default();
                        for (source, amount) in source_map {
                            *combined_target_map.entry(source.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate damage by target, source, and type dealt
                    for (target, source_map) in &stats.damage_by_target_source_and_type_dealt {
                        let combined_target_map = combined.damage_by_target_source_and_type_dealt.entry(target.clone()).or_default();
                        for (source, type_map) in source_map {
                            let combined_source_map = combined_target_map.entry(source.clone()).or_default();
                            for (dtype, amount) in type_map {
                                *combined_source_map.entry(dtype.clone()).or_default() += *amount;
                            }
                        }
                    }
                    
                    // Aggregate hit damage by target and type
                    for (target, type_map) in &stats.hit_damage_by_target_type {
                        let combined_target_map = combined.hit_damage_by_target_type.entry(target.clone()).or_default();
                        for (dtype, amount) in type_map {
                            *combined_target_map.entry(dtype.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate crit damage by target and type
                    for (target, type_map) in &stats.crit_damage_by_target_type {
                        let combined_target_map = combined.crit_damage_by_target_type.entry(target.clone()).or_default();
                        for (dtype, amount) in type_map {
                            *combined_target_map.entry(dtype.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate weapon buff damage by target and type
                    for (target, type_map) in &stats.weapon_buff_damage_by_target_type {
                        let combined_target_map = combined.weapon_buff_damage_by_target_type.entry(target.clone()).or_default();
                        for (dtype, amount) in type_map {
                            *combined_target_map.entry(dtype.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate damage sources received
                    for (source, amount) in &stats.damage_by_source_received {
                        *combined.damage_by_source_received.entry(source.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate damage by source and type received
                    for (source, type_map) in &stats.damage_by_source_and_type_received {
                        let combined_source_map = combined.damage_by_source_and_type_received.entry(source.clone()).or_default();
                        for (dtype, amount) in type_map {
                            *combined_source_map.entry(dtype.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate damage by attacker received
                    for (attacker, amount) in &stats.damage_by_attacker_received {
                        *combined.damage_by_attacker_received.entry(attacker.clone()).or_default() += *amount;
                    }
                    
                    // Aggregate damage by attacker and source received
                    for (attacker, source_map) in &stats.damage_by_attacker_and_source_received {
                        let combined_attacker_map = combined.damage_by_attacker_and_source_received.entry(attacker.clone()).or_default();
                        for (source, amount) in source_map {
                            *combined_attacker_map.entry(source.clone()).or_default() += *amount;
                        }
                    }
                    
                    // Aggregate absorbed damage by type
                    for (dtype, amount) in &stats.absorbed_by_type {
                        *combined.absorbed_by_type.entry(dtype.clone()).or_default() += *amount;
                    }
                    
                    // Update timing for combined stats
                    if let Some(first) = stats.first_action_time {
                        combined.first_action_time = Some(
                            combined.first_action_time.map_or(first, |existing| existing.min(first))
                        );
                    }
                    if let Some(last) = stats.last_action_time {
                        combined.last_action_time = Some(
                            combined.last_action_time.map_or(last, |existing| existing.max(last))
                        );
                    }
                }
            }
        }
        
        combined_stats
    }

    fn get_overall_stats_safe(&self) -> HashMap<String, CombatantStats> {
        if let Ok(encounters) = self.encounters.try_lock() {
            self.combine_all_encounters_stats(&encounters)
        } else {
            HashMap::new()
        }
    }

    fn get_overall_stats(&self) -> HashMap<String, CombatantStats> {
        let encounters = self.encounters.lock().unwrap();
        self.combine_all_encounters_stats(&encounters)
    }

    fn combine_all_encounters_stats(&self, encounters: &HashMap<u64, Encounter>) -> HashMap<String, CombatantStats> {
        let mut overall_stats = HashMap::new();
        
        for encounter in encounters.values() {
            for (name, stats) in &encounter.stats {
                let overall = overall_stats.entry(name.clone()).or_insert_with(CombatantStats::default);
                
                // Aggregate all stats
                overall.hits += stats.hits;
                overall.misses += stats.misses;
                overall.critical_hits += stats.critical_hits;
                overall.weapon_buffs += stats.weapon_buffs;
                overall.total_damage_dealt += stats.total_damage_dealt;
                overall.hit_damage += stats.hit_damage;
                overall.crit_damage += stats.crit_damage;
                overall.weapon_buff_damage += stats.weapon_buff_damage;
                
                // Aggregate hit damage by type
                for (dtype, amount) in &stats.hit_damage_by_type {
                    *overall.hit_damage_by_type.entry(dtype.clone()).or_default() += *amount;
                }
                
                // Aggregate crit damage by type
                for (dtype, amount) in &stats.crit_damage_by_type {
                    *overall.crit_damage_by_type.entry(dtype.clone()).or_default() += *amount;
                }
                
                // Aggregate weapon buff damage by type
                for (dtype, amount) in &stats.weapon_buff_damage_by_type {
                    *overall.weapon_buff_damage_by_type.entry(dtype.clone()).or_default() += *amount;
                }
                overall.times_attacked += stats.times_attacked;
                overall.total_damage_received += stats.total_damage_received;
                overall.total_damage_absorbed += stats.total_damage_absorbed;
                
                // Aggregate damage by type dealt
                for (dtype, amount) in &stats.damage_by_type_dealt {
                    *overall.damage_by_type_dealt.entry(dtype.clone()).or_default() += *amount;
                }
                
                // Aggregate damage by source dealt
                for (source, amount) in &stats.damage_by_source_dealt {
                    *overall.damage_by_source_dealt.entry(source.clone()).or_default() += *amount;
                }
                
                // Aggregate damage by source and type dealt
                for (source, type_map) in &stats.damage_by_source_and_type_dealt {
                    let overall_source_map = overall.damage_by_source_and_type_dealt.entry(source.clone()).or_default();
                    for (dtype, amount) in type_map {
                        *overall_source_map.entry(dtype.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate damage by target dealt
                for (target, amount) in &stats.damage_by_target_dealt {
                    *overall.damage_by_target_dealt.entry(target.clone()).or_default() += *amount;
                }
                
                // Aggregate damage by target and source dealt
                for (target, source_map) in &stats.damage_by_target_and_source_dealt {
                    let overall_target_map = overall.damage_by_target_and_source_dealt.entry(target.clone()).or_default();
                    for (source, amount) in source_map {
                        *overall_target_map.entry(source.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate damage by target, source, and type dealt
                for (target, source_map) in &stats.damage_by_target_source_and_type_dealt {
                    let overall_target_map = overall.damage_by_target_source_and_type_dealt.entry(target.clone()).or_default();
                    for (source, type_map) in source_map {
                        let overall_source_map = overall_target_map.entry(source.clone()).or_default();
                        for (dtype, amount) in type_map {
                            *overall_source_map.entry(dtype.clone()).or_default() += *amount;
                        }
                    }
                }
                
                // Aggregate hit damage by target and type
                for (target, type_map) in &stats.hit_damage_by_target_type {
                    let overall_target_map = overall.hit_damage_by_target_type.entry(target.clone()).or_default();
                    for (dtype, amount) in type_map {
                        *overall_target_map.entry(dtype.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate crit damage by target and type
                for (target, type_map) in &stats.crit_damage_by_target_type {
                    let overall_target_map = overall.crit_damage_by_target_type.entry(target.clone()).or_default();
                    for (dtype, amount) in type_map {
                        *overall_target_map.entry(dtype.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate weapon buff damage by target and type
                for (target, type_map) in &stats.weapon_buff_damage_by_target_type {
                    let overall_target_map = overall.weapon_buff_damage_by_target_type.entry(target.clone()).or_default();
                    for (dtype, amount) in type_map {
                        *overall_target_map.entry(dtype.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate damage by type received
                for (dtype, amount) in &stats.damage_by_type_received {
                    *overall.damage_by_type_received.entry(dtype.clone()).or_default() += *amount;
                }
                
                // Aggregate damage by source received
                for (source, amount) in &stats.damage_by_source_received {
                    *overall.damage_by_source_received.entry(source.clone()).or_default() += *amount;
                }
                
                // Aggregate damage by source and type received
                for (source, type_map) in &stats.damage_by_source_and_type_received {
                    let overall_source_map = overall.damage_by_source_and_type_received.entry(source.clone()).or_default();
                    for (dtype, amount) in type_map {
                        *overall_source_map.entry(dtype.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate damage by attacker received
                for (attacker, amount) in &stats.damage_by_attacker_received {
                    *overall.damage_by_attacker_received.entry(attacker.clone()).or_default() += *amount;
                }
                
                // Aggregate damage by attacker and source received
                for (attacker, source_map) in &stats.damage_by_attacker_and_source_received {
                    let overall_attacker_map = overall.damage_by_attacker_and_source_received.entry(attacker.clone()).or_default();
                    for (source, amount) in source_map {
                        *overall_attacker_map.entry(source.clone()).or_default() += *amount;
                    }
                }
                
                // Aggregate absorption by type
                for (dtype, amount) in &stats.absorbed_by_type {
                    *overall.absorbed_by_type.entry(dtype.clone()).or_default() += *amount;
                }
            }
        }
        
        overall_stats
    }
    
    fn compute_stats_hash(stats_map: &HashMap<String, CombatantStats>) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Create a sorted vector of the data for consistent hashing
        let mut items: Vec<_> = stats_map.iter().collect();
        items.sort_by_key(|(name, _)| name.as_str());
        
        for (name, stats) in items {
            name.hash(&mut hasher);
            stats.total_damage_dealt.hash(&mut hasher);
            stats.total_damage_received.hash(&mut hasher);
            stats.total_damage_absorbed.hash(&mut hasher);
            stats.hits.hash(&mut hasher);
            stats.misses.hash(&mut hasher);
            stats.critical_hits.hash(&mut hasher);
            stats.times_attacked.hash(&mut hasher);
        }
        
        hasher.finish()
    }
    
    fn update_sorted_cache(&mut self, stats_map: &HashMap<String, CombatantStats>) {
        let current_hash = Self::compute_stats_hash(stats_map);
        
        // Only re-sort if the data has changed
        if current_hash != self.last_data_hash {
            self.cached_sorted_combatants = stats_map.iter()
                .map(|(name, stats)| (name.clone(), stats.clone()))
                .collect();
            
            // Sort by: 1) total damage dealt (desc), 2) total damage received (desc), 3) name (asc)
            self.cached_sorted_combatants.sort_by(|a, b| {
                b.1.total_damage_dealt.cmp(&a.1.total_damage_dealt)
                    .then(b.1.total_damage_received.cmp(&a.1.total_damage_received))
                    .then(a.0.cmp(&b.0))
            });
            
            self.last_data_hash = current_hash;
        }
    }
    
    fn display_stats(&mut self, ui: &mut egui::Ui, stats_map: &HashMap<String, CombatantStats>) {
        // Update cache only if data changed
        self.update_sorted_cache(stats_map);
        
        // Calculate total encounter damage for percentage calculation
        let total_encounter_damage: u32 = stats_map.values().map(|s| s.total_damage_dealt).sum();
        
        // Find the maximum damage for bar scaling
        let max_damage = stats_map.values().map(|s| s.total_damage_dealt).max().unwrap_or(1);
        
        // Use scrollable area and collapsible headers that scales with window size
        let available_height = ui.available_height().max(200.0);
        egui::ScrollArea::both()
            .max_height(available_height - 20.0) // Leave some padding
            .auto_shrink([false; 2])
            .show(ui, |ui| {
            for (name, stats) in &self.cached_sorted_combatants {
                // Calculate damage percentage for this player
                let damage_percentage = if total_encounter_damage > 0 && stats.total_damage_dealt > 0 {
                    (stats.total_damage_dealt as f32 / total_encounter_damage as f32 * 100.0) as u32
                } else {
                    0
                };
                
                // Custom collapsible header with right-aligned damage info and progress bar
                let id = egui::Id::new(name);
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
                
                // Calculate bar width based on damage relative to max damage
                let bar_percentage = if max_damage > 0 {
                    stats.total_damage_dealt as f32 / max_damage as f32
                } else {
                    0.0
                };
                
                // Reserve space for the header
                let header_height = 24.0;
                let available_width = ui.available_width();
                let header_rect = ui.allocate_space(egui::Vec2::new(available_width, header_height)).1;
                
                // Draw the red progress bar background
                if bar_percentage > 0.0 {
                    let bar_width = available_width * bar_percentage;
                    let bar_rect = egui::Rect::from_min_size(
                        header_rect.min,
                        egui::Vec2::new(bar_width, header_height)
                    );
                    ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(200, 50, 50));
                }
                
                // Create a clickable header that covers the entire area
                let header_response = ui.allocate_rect(header_rect, egui::Sense::click());
                
                // Draw the header content on top of the bar (non-interactive)
                let text_painter = ui.painter();
                
                // Draw collapsing arrow
                let arrow_text = if state.is_open() { "▼" } else { "▶" };
                let arrow_pos = egui::Pos2::new(header_rect.min.x + 8.0, header_rect.center().y - 6.0);
                text_painter.text(arrow_pos, egui::Align2::LEFT_CENTER, arrow_text, egui::FontId::default(), egui::Color32::WHITE);
                
                // Draw player name
                let name_pos = egui::Pos2::new(header_rect.min.x + 25.0, header_rect.center().y);
                text_painter.text(name_pos, egui::Align2::LEFT_CENTER, name.clone(), 
                    egui::FontId::proportional(14.0), egui::Color32::WHITE);
                
                // Draw damage info on the right
                if stats.total_damage_dealt > 0 {
                    let damage_info = if let Some(dps) = stats.calculate_dps() {
                        format!("{} ({:.1}, {}%)", stats.total_damage_dealt, dps, damage_percentage)
                    } else {
                        format!("{} ({}%)", stats.total_damage_dealt, damage_percentage)
                    };
                    let damage_pos = egui::Pos2::new(header_rect.max.x - 8.0, header_rect.center().y);
                    text_painter.text(damage_pos, egui::Align2::RIGHT_CENTER, damage_info, 
                        egui::FontId::proportional(14.0), egui::Color32::WHITE);
                }
                
                // Handle click to toggle
                if header_response.clicked() {
                    state.toggle(ui);
                }
                
                state.store(ui.ctx());
                
                // Show content if expanded
                if state.is_open() {
                    ui.indent(id, |ui| {
                        if stats.total_damage_dealt > 0 {
                            let damage_label = if let Some(dps) = stats.calculate_dps() {
                                format!("Total Damage Dealt: {} ({:.1} DPS)", stats.total_damage_dealt, dps)
                            } else {
                                format!("Total Damage Dealt: {}", stats.total_damage_dealt)
                            };
                            
                            self.custom_collapsing_header(ui, egui::Id::new(format!("{}_damage_dealt", name)), &damage_label, |ui| {
                                    // Sort targets by damage dealt to them (highest to lowest)
                                    let mut sorted_targets: Vec<_> = stats.damage_by_target_dealt.iter().collect();
                                    sorted_targets.sort_by(|a, b| b.1.cmp(a.1));
                                    
                                    for (target, target_damage) in sorted_targets {
                                        // Show each target as a collapsible header with total damage to that target
                                        let target_dps_text = if let Some(dps) = stats.calculate_source_dps(*target_damage) {
                                            format!("{}: {} ({:.1} DPS)", target, target_damage, dps)
                                        } else {
                                            format!("{}: {}", target, target_damage)
                                        };
                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_target_{}", name, target)), &target_dps_text, |ui| {
                                            // Show damage sources for this specific target, sorted by amount
                                            if let Some(source_map) = stats.damage_by_target_and_source_dealt.get(target) {
                                                let mut sorted_sources: Vec<_> = source_map.iter().collect();
                                                // Sort with Attack first, then spells alphabetically
                                                sorted_sources.sort_by(|a, b| {
                                                    if a.0 == "Attack" && b.0 != "Attack" {
                                                        std::cmp::Ordering::Less
                                                    } else if a.0 != "Attack" && b.0 == "Attack" {
                                                        std::cmp::Ordering::Greater
                                                    } else {
                                                        a.0.cmp(b.0)
                                                    }
                                                });
                                                
                                                for (source, amount) in sorted_sources {
                                                    let source_dps_text = if let Some(dps) = stats.calculate_source_dps(*amount) {
                                                        format!("{}: {} ({:.1} DPS)", source, amount, dps)
                                                    } else {
                                                        format!("{}: {}", source, amount)
                                                    };
                                                    self.custom_collapsing_header(ui, egui::Id::new(format!("{}_target_{}_source_{}", name, target, source)), &source_dps_text, |ui| {
                                                // Show detailed attack statistics if this is the "Attack" source
                                                if source == "Attack" {
                                                    // Calculate accurate average damage for hits and crits
                                                    let hit_avg = if stats.hits > 0 { stats.hit_damage / stats.hits } else { 0 };
                                                    let crit_avg = if stats.critical_hits > 0 { stats.crit_damage / stats.critical_hits } else { 0 };
                                                    
                                                    if stats.misses > 0 {
                                                        if stats.concealment_dodges > 0 {
                                                            ui.label(format!("Misses {} (concealment {})", stats.misses, stats.concealment_dodges));
                                                        } else {
                                                            ui.label(format!("{} Misses", stats.misses));
                                                        }
                                                    }
                                                    
                                                    if stats.hits > 0 {
                                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_hits", name)), &format!("{} Hits (avg {})", stats.hits, hit_avg), |ui| {
                                                                if let Some(type_map) = stats.hit_damage_by_target_type.get(target) {
                                                                    for (damage_type, type_amount) in type_map {
                                                                        ui.label(format!("{}: {}", damage_type, type_amount));
                                                                    }
                                                                }
                                                            });
                                                    }
                                                    
                                                    if stats.critical_hits > 0 {
                                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_crits", name)), &format!("{} Crits (avg {})", stats.critical_hits, crit_avg), |ui| {
                                                                if let Some(type_map) = stats.crit_damage_by_target_type.get(target) {
                                                                    for (damage_type, type_amount) in type_map {
                                                                        ui.label(format!("{}: {}", damage_type, type_amount));
                                                                    }
                                                                }
                                                            });
                                                    }
                                                    
                                                    if stats.weapon_buffs > 0 {
                                                        let buff_avg = if stats.weapon_buffs > 0 { stats.weapon_buff_damage / stats.weapon_buffs } else { 0 };
                                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_weapon_buffs", name)), &format!("{} Weapon Buff (avg {})", stats.weapon_buffs, buff_avg), |ui| {
                                                                if let Some(type_map) = stats.weapon_buff_damage_by_target_type.get(target) {
                                                                    for (damage_type, type_amount) in type_map {
                                                                        ui.label(format!("{}: {}", damage_type, type_amount));
                                                                    }
                                                                }
                                                            });
                                                    }
                                                } else {
                                                    // For non-Attack sources, show damage types for this specific target
                                                    if let Some(target_map) = stats.damage_by_target_source_and_type_dealt.get(target) {
                                                        if let Some(type_map) = target_map.get(source) {
                                                            for (damage_type, type_amount) in type_map {
                                                                ui.label(format!("{}: {}", damage_type, type_amount));
                                                            }
                                                        }
                                                    }
                                                    }
                                                });
                                                }
                                            }
                                        });
                                    }
                                });
                        }
                        if stats.total_damage_received > 0 || stats.total_damage_absorbed > 0 {
                            self.custom_collapsing_header(ui, egui::Id::new(format!("{}_damage_received", name)), &format!("Total Damage Received: {}", stats.total_damage_received), |ui| {
                                    // Sort attackers by damage received from them (highest to lowest)
                                    let mut sorted_attackers: Vec<_> = stats.damage_by_attacker_received.iter().collect();
                                    sorted_attackers.sort_by(|a, b| b.1.cmp(a.1));
                                    
                                    for (attacker, attacker_damage) in sorted_attackers {
                                        // Show each attacker as a collapsible header with total damage from that attacker
                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_attacker_{}", name, attacker)), &format!("{}: {}", attacker, attacker_damage), |ui| {
                                            // Show damage sources from this specific attacker, sorted by amount
                                            if let Some(source_map) = stats.damage_by_attacker_and_source_received.get(attacker) {
                                                let mut sorted_sources: Vec<_> = source_map.iter().collect();
                                                sorted_sources.sort_by(|a, b| b.1.cmp(a.1));
                                                
                                                for (source, amount) in sorted_sources {
                                                    self.custom_collapsing_header(ui, egui::Id::new(format!("{}_attacker_{}_source_{}", name, attacker, source)), &format!("{}: {}", source, amount), |ui| {
                                                        // Show damage types for this attacker's source
                                                        let combined_source = format!("{} ({})", attacker, source);
                                                        if let Some(type_map) = stats.damage_by_source_and_type_received.get(&combined_source) {
                                                            for (damage_type, type_amount) in type_map {
                                                                let absorbed_amount = stats.absorbed_by_type.get(damage_type).unwrap_or(&0);
                                                                let total_attempted_type = type_amount + absorbed_amount;
                                                                
                                                                if *absorbed_amount > 0 {
                                                                    let immunity_percent = if total_attempted_type > 0 {
                                                                        (*absorbed_amount as f32 / total_attempted_type as f32 * 100.0) as u32
                                                                    } else {
                                                                        0
                                                                    };
                                                                    ui.label(format!("{}: {} ({} absorbed, {}% immunity)", 
                                                                        damage_type, type_amount, absorbed_amount, immunity_percent));
                                                                } else {
                                                                    ui.label(format!("{}: {}", damage_type, type_amount));
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                        });
                                    }
                                    
                                    // Show absorbed damage types that had no received damage (100% immunity)
                                    if !stats.absorbed_by_type.is_empty() {
                                        let mut all_received_types = std::collections::HashSet::new();
                                        for type_map in stats.damage_by_source_and_type_received.values() {
                                            for damage_type in type_map.keys() {
                                                all_received_types.insert(damage_type.clone());
                                            }
                                        }
                                        
                                        for (damage_type, absorbed_amount) in &stats.absorbed_by_type {
                                            if !all_received_types.contains(damage_type) {
                                                ui.label(format!("Complete Immunity - {}: 0 ({} absorbed, 100% immunity)", 
                                                    damage_type, absorbed_amount));
                                            }
                                        }
                                    }
                                });
                        }
                    });
                }
            }
        });
    }
}

impl eframe::App for NwnLogApp {
    /// This function is called on every frame to update the GUI.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request continuous repaints to keep the app updating even when not focused
        ctx.request_repaint();
        egui::CentralPanel::default().show(ctx, |ui| {
            // Prevent the UI from auto-sizing by setting a minimum size
            ui.set_min_size(egui::Vec2::new(300.0, 200.0));
            
            // Custom header bar
            let header_rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), 35.0)).1;
            
            // Make the entire header draggable except for the buttons
            let draggable_rect = egui::Rect::from_min_size(
                header_rect.min,
                egui::Vec2::new(header_rect.width() - 140.0, header_rect.height()) // More space for font buttons
            );
            let drag_response = ui.allocate_rect(draggable_rect, egui::Sense::click_and_drag());
            if drag_response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            
            // Draw the header content
            ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                ui.horizontal(|ui| {
                    // Left-aligned title (draggable area) - draw as non-interactive text
                    let title_pos = egui::Pos2::new(header_rect.min.x + 15.0, header_rect.center().y);
                    ui.painter().text(title_pos, egui::Align2::LEFT_CENTER, "NWN Combat Tracker", 
                        egui::FontId::proportional(16.0), ui.visuals().text_color());
                    
                    // Push buttons to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        if ui.add(egui::Button::new(egui::RichText::new("✕").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        
                        // Minimize button  
                        if ui.add(egui::Button::new(egui::RichText::new("−").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        
                        // Add gap between minimize/close and font buttons
                        ui.add_space(10.0);
                        
                        // Font size decrease button (smaller A)
                        if ui.add(egui::Button::new(egui::RichText::new("A").size(8.0))
                            .min_size(egui::Vec2::new(18.0, 18.0))).clicked() {
                            self.text_scale = (self.text_scale - 0.1).max(0.5);
                            ctx.set_zoom_factor(ctx.zoom_factor() * 0.9);
                        }
                        
                        // Font size increase button (large A) - placed closer to small A
                        if ui.add(egui::Button::new(egui::RichText::new("A").size(16.0))
                            .min_size(egui::Vec2::new(20.0, 20.0))).clicked() {
                            self.text_scale = (self.text_scale + 0.1).min(2.0);
                            ctx.set_zoom_factor(ctx.zoom_factor() * 1.1);
                        }
                    });
                });
            });
            
            ui.separator();
            
            // View mode selector
            ui.horizontal(|ui| {
                // Fixed width buttons for view modes
                // Determine what data is currently being shown to highlight the correct button
                let showing_encounters = !self.selected_encounter_ids.is_empty();
                let showing_current_fight = self.selected_encounter_ids.is_empty() && self.view_mode == ViewMode::CurrentFight;
                let showing_overall_stats = self.selected_encounter_ids.is_empty() && self.view_mode == ViewMode::OverallStats;
                
                if ui.add_sized([100.0, 20.0], egui::Button::new("Current Fight").selected(showing_current_fight)).clicked() {
                    self.view_mode = ViewMode::CurrentFight;
                    self.selected_encounter_ids.clear(); // Clear encounter selections when switching to Current Fight
                }
                if ui.add_sized([100.0, 20.0], egui::Button::new("Overall Stats").selected(showing_overall_stats)).clicked() {
                    self.view_mode = ViewMode::OverallStats;
                    self.selected_encounter_ids.clear(); // Clear encounter selections when switching to Overall Stats
                }
                let encounters_button_text = if !self.selected_encounter_ids.is_empty() {
                    format!("Encounters ({})", self.selected_encounter_ids.len())
                } else {
                    "Encounters".to_string()
                };
                if ui.add_sized([120.0, 20.0], egui::Button::new(encounters_button_text).selected(showing_encounters || self.view_mode == ViewMode::MultipleSelected)).clicked() {
                    if self.view_mode == ViewMode::MultipleSelected {
                        // If already in multi-select mode, close the selection UI but keep showing the data
                        self.view_mode = ViewMode::CurrentFight; // This will be overridden by get_current_stats if encounters are selected
                    } else {
                        // Switch to multi-select mode to show the selection UI
                        self.view_mode = ViewMode::MultipleSelected;
                    }
                }
            });
            
            // Show encounter selection UI if in multi-select mode
            if self.view_mode == ViewMode::MultipleSelected {
                ui.separator();
                ui.label("Select encounters to combine:");
                
                egui::ScrollArea::vertical()
                    .id_salt("encounter_selection_scroll")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        if let Ok(encounters) = self.encounters.try_lock() {
                            let mut sorted_encounters: Vec<_> = encounters.values().collect();
                            sorted_encounters.sort_by(|a, b| b.end_time.cmp(&a.end_time));
                            
                            for encounter in sorted_encounters {
                                let mut is_selected = self.selected_encounter_ids.contains(&encounter.id);
                                let display_name = encounter.get_display_name();
                                
                                // Make the entire row clickable by using a horizontal layout
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut is_selected, "").changed() {
                                        if is_selected {
                                            self.selected_encounter_ids.insert(encounter.id);
                                        } else {
                                            self.selected_encounter_ids.remove(&encounter.id);
                                        }
                                    }
                                    
                                    // Make the text also clickable
                                    let text_response = ui.selectable_label(is_selected, display_name);
                                    if text_response.clicked() {
                                        if is_selected {
                                            self.selected_encounter_ids.remove(&encounter.id);
                                        } else {
                                            self.selected_encounter_ids.insert(encounter.id);
                                        }
                                    }
                                });
                            }
                        }
                    });
                
                // Show selection summary
                if !self.selected_encounter_ids.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Selected {} encounter(s)", self.selected_encounter_ids.len()));
                        
                        if ui.button("Clear All").clicked() {
                            self.selected_encounter_ids.clear();
                        }
                        if ui.button("Select All").clicked() {
                            if let Ok(encounters) = self.encounters.try_lock() {
                                for encounter_id in encounters.keys() {
                                    self.selected_encounter_ids.insert(*encounter_id);
                                }
                            }
                        }
                    });
                }
            }
            
            ui.separator();
            
            // Get the stats data using the new system
            let stats_to_display = Some(self.get_current_stats());
            
            // Now display the UI with the collected data
            if let Some(stats_map) = stats_to_display {
                self.display_stats(ui, &stats_map);
            }
            
            // Simple resize grip using a different approach
            let window_rect = ui.max_rect();
            let grip_size = 10.0;
            
            // Bottom-right corner resize grip only
            let grip_rect = egui::Rect::from_min_size(
                egui::Pos2::new(window_rect.max.x - grip_size, window_rect.max.y - grip_size),
                egui::Vec2::new(grip_size, grip_size)
            );
            
            // Use interact instead of allocate_rect for better behavior
            let grip_id = egui::Id::new("resize_grip");
            let grip_response = ui.interact(grip_rect, grip_id, egui::Sense::click_and_drag());
            
            if grip_response.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeSouthEast);
            }
            
            if grip_response.dragged() {
                let delta = grip_response.drag_delta();
                if delta.length() > 0.1 { // More sensitive to small movements
                    // Get current window size more reliably
                    let current_size = ctx.input(|i| {
                        if let Some(rect) = i.viewport().inner_rect {
                            rect.size()
                        } else {
                            // Fallback to screen rect size
                            i.screen_rect().size()
                        }
                    });
                    
                    let new_width = (current_size.x + delta.x).clamp(300.0, 1600.0);
                    let new_height = (current_size.y + delta.y).clamp(200.0, 1200.0);
                    
                    // Send the resize command
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(new_width, new_height)));
                    
                    // Force a repaint to make the resize more responsive
                    ctx.request_repaint();
                }
            }
            
            // Draw resize grip indicator
            let grip_color = if grip_response.hovered() {
                egui::Color32::from_gray(150)
            } else {
                egui::Color32::from_gray(100)
            };
            
            // Draw resize grip lines
            let painter = ui.painter();
            for i in 0..3 {
                let offset = i as f32 * 2.5;
                let start = egui::Pos2::new(grip_rect.max.x - 1.0 - offset, grip_rect.min.y + offset + 1.0);
                let end = egui::Pos2::new(grip_rect.min.x + offset + 1.0, grip_rect.max.y - 1.0 - offset);
                painter.line_segment([start, end], egui::Stroke::new(1.0, grip_color));
            }
            
        });
    }
    
    /// Keep window background opaque and visible.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.2, 0.2, 0.2, 1.0] // Dark gray, fully opaque
    }
}

// --- Core Log Parsing Logic ---

/// Tracks pending spell effects to associate with incoming damage
#[derive(Debug, Clone)]
struct SpellContext {
    caster: String,
    spell: String,
    affected_targets: Vec<String>,
    timestamp: u64,
}

/// Tracks pending attacks waiting for damage
#[derive(Debug, Clone)]
struct PendingAttack {
    attacker: String,
    target: String,
    timestamp: u64,
    is_crit: bool,
}

/// Tracks pending spells waiting for damage
#[derive(Debug, Clone)]
struct PendingSpell {
    caster: String,
    target: String,
    spell: String,
    timestamp: u64,
    had_save_roll: bool, // Track if this spell had a save roll after spell resist
    had_damage_immunity: bool, // Track if this spell had damage immunity absorption after spell resist
}

/// Tracks long-duration spells (missile storms) that last 6 seconds
#[derive(Debug, Clone)]
struct LongDurationSpell {
    caster: String,
    target: String,
    spell: String,
    timestamp: u64,
    had_save_roll: bool,
    had_damage_immunity: bool,
}

lazy_static! {
    static ref RE_ATTACK: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : (?:\*target concealed: (?P<concealment>\d+)%\* : )?\*(?P<result>hit|miss|critical hit)\*").unwrap();
    static ref RE_CONCEALMENT: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : \*target concealed: (?P<concealment>\d+)%\* : \(.+\)").unwrap();
    static ref RE_DAMAGE: Regex = Regex::new(r"^(?P<attacker>.+?) damages (?P<target>.+?): (?P<total>\d+) \((?P<breakdown>.+)\)").unwrap();
    static ref RE_ABSORB: Regex = Regex::new(r"^(?P<target>.+?) : Damage Immunity absorbs (?P<amount>\d+) point\(s\) of (?P<type>\w+)").unwrap();
    static ref RE_TIMESTAMP: Regex = Regex::new(r"^\[CHAT WINDOW TEXT\] \[([^\]]+)\]").unwrap();
    static ref RE_SPELL_RESIST: Regex = Regex::new(r"^SPELL RESIST: (?P<target>.+?) attempts to resist: (?P<spell>.+?) - Result:\s+(?P<result>FAILED|SUCCESS)").unwrap();
    static ref RE_SAVE: Regex = Regex::new(r"^SAVE: (?P<target>.+?) : (?P<save_type>.+?) vs\. (?P<element>.+?) : \*(?P<result>failed|succeeded)\*").unwrap();
    static ref RE_CASTING: Regex = Regex::new(r"^(?P<caster>.+?) casting (?P<spell>.+)").unwrap();
    static ref RE_CASTS: Regex = Regex::new(r"^(?P<caster>.+?) casts (?P<spell>.+)").unwrap();
}

enum ParsedLine {
    Attack { attacker: String, target: String, result: String, concealment: bool, timestamp: u64 },
    Damage { attacker: String, target: String, total: u32, breakdown: HashMap<String, u32>, timestamp: u64 },
    Absorb { target: String, amount: u32, dtype: String, timestamp: u64 },
    SpellResist { target: String, spell: String, result: String, timestamp: u64 },
    Save { target: String, save_type: String, element: String, result: String, timestamp: u64 },
    Casting { caster: String, spell: String, timestamp: u64 },
    Casts { caster: String, spell: String, timestamp: u64 },
}

fn is_long_duration_spell(spell: &str) -> bool {
    matches!(spell, 
        "Isaac's Greater Missile Storm" | 
        "Isaac's Lesser Missile Storm" | 
        "Magic Missile" | 
        "Flame Arrow" | 
        "Ball Lightning"
    )
}

fn get_spell_damage_type(spell: &str) -> Option<&'static str> {
    match spell {
        "Flame Arrow" => Some("Fire"),
        "Ball Lightning" => Some("Electrical"),
        "Isaac's Greater Missile Storm" | "Isaac's Lesser Missile Storm" | "Magic Missile" => Some("Magical"),
        _ => None,
    }
}

fn parse_timestamp(timestamp_str: &str) -> u64 {
    // NWN timestamp format is like "Tue Jul 29 14:10:26"
    // Parse the time components and convert to seconds since start of day
    if let Some(time_part) = timestamp_str.split_whitespace().nth(3) {
        let parts: Vec<&str> = time_part.split(':').collect();
        if parts.len() == 3 {
            if let (Ok(hours), Ok(minutes), Ok(seconds)) = (
                parts[0].parse::<u64>(),
                parts[1].parse::<u64>(),
                parts[2].parse::<u64>()
            ) {
                return hours * 3600 + minutes * 60 + seconds;
            }
        }
    }
    
    // Fallback: use hash if parsing fails
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    timestamp_str.hash(&mut hasher);
    (hasher.finish() % (u32::MAX as u64)) + 1000000
}

fn parse_log_line(line: &str) -> Option<ParsedLine> {
    let timestamp = if let Some(caps) = RE_TIMESTAMP.captures(line) {
        parse_timestamp(&caps[1])
    } else {
        get_current_timestamp() // Fallback to current time if no timestamp
    };
    
    let clean_line = line.trim().strip_prefix("[CHAT WINDOW TEXT]").and_then(|s| s.splitn(2, ']').nth(1)).unwrap_or(line).trim();

    if let Some(caps) = RE_SPELL_RESIST.captures(clean_line) {
        return Some(ParsedLine::SpellResist {
            target: caps["target"].trim().to_string(),
            spell: caps["spell"].trim().to_string(),
            result: caps["result"].to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_SAVE.captures(clean_line) {
        return Some(ParsedLine::Save {
            target: caps["target"].trim().to_string(),
            save_type: caps["save_type"].trim().to_string(),
            element: caps["element"].trim().to_string(),
            result: caps["result"].to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_CASTING.captures(clean_line) {
        return Some(ParsedLine::Casting {
            caster: caps["caster"].trim().to_string(),
            spell: caps["spell"].trim().to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_CASTS.captures(clean_line) {
        return Some(ParsedLine::Casts {
            caster: caps["caster"].trim().to_string(),
            spell: caps["spell"].trim().to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_ATTACK.captures(clean_line) {
        let concealment = caps.name("concealment").is_some();
        return Some(ParsedLine::Attack {
            attacker: caps["attacker"].trim().to_string(),
            target: caps["target"].trim().to_string(),
            result: caps["result"].to_string(),
            concealment,
            timestamp,
        });
    }

    // Check for concealment attacks (which are actually misses according to user clarification)
    if let Some(caps) = RE_CONCEALMENT.captures(clean_line) {
        return Some(ParsedLine::Attack {
            attacker: caps["attacker"].trim().to_string(),
            target: caps["target"].trim().to_string(),
            result: "miss".to_string(),
            concealment: true,
            timestamp,
        });
    }

    if let Some(caps) = RE_DAMAGE.captures(clean_line) {
        let mut damage_breakdown = HashMap::new();
        let parts = caps["breakdown"].split_whitespace().collect::<Vec<_>>();
        for chunk in parts.chunks(2) {
            if let (Ok(amount), Some(dtype)) = (chunk[0].parse::<u32>(), chunk.get(1)) {
                damage_breakdown.insert(dtype.to_string(), amount);
            }
        }
        return Some(ParsedLine::Damage {
            attacker: caps["attacker"].trim().to_string(),
            target: caps["target"].trim().to_string(),
            total: caps["total"].parse().unwrap_or(0),
            breakdown: damage_breakdown,
            timestamp,
        });
    }

    if let Some(caps) = RE_ABSORB.captures(clean_line) {
        return Some(ParsedLine::Absorb {
            target: caps["target"].trim().to_string(),
            amount: caps["amount"].parse().unwrap_or(0),
            dtype: caps["type"].to_string(),
            timestamp,
        });
    }

    None
}

fn find_latest_log_file_in_dir(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            path.is_file() && path.file_name().and_then(|s| s.to_str())
                .map_or(false, |s| s.starts_with("nwclientLog") && s.ends_with(".txt"))
        })
        .max_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|entry| entry.path())
}

fn find_latest_log_file() -> Option<PathBuf> {
    if cfg!(windows) {
        // Try OneDrive path first
        let onedrive_path = get_onedrive_logs_path();
        if let Some(log_file) = find_latest_log_file_in_dir(&onedrive_path) {
            return Some(log_file);
        }
        
        // Try regular Documents path
        let regular_path = get_regular_logs_path();
        if let Some(log_file) = find_latest_log_file_in_dir(&regular_path) {
            return Some(log_file);
        }
        
        None
    } else {
        // Unix-like systems: use existing logic
        let log_dir = get_unix_logs_path();
        find_latest_log_file_in_dir(&log_dir)
    }
}

fn get_onedrive_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    if let Ok(home) = std::env::var("USERPROFILE") {
        path.push(home);
    } else {
        path.push("C:\\Users");
        if let Ok(username) = std::env::var("USERNAME") {
            path.push(username);
        } else {
            path.push("Default");
        }
    }
    path.push("OneDrive");
    path.push("Documents");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

fn get_regular_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    if let Ok(home) = std::env::var("USERPROFILE") {
        path.push(home);
    } else {
        path.push("C:\\Users");
        if let Ok(username) = std::env::var("USERNAME") {
            path.push(username);
        } else {
            path.push("Default");
        }
    }
    path.push("Documents");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

fn get_unix_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    if let Ok(home) = std::env::var("HOME") {
        path.push(home);
    } else {
        path.push("/home");
        if let Ok(user) = std::env::var("USER") {
            path.push(user);
        } else {
            path.push("default");
        }
    }
    path.push(".local");
    path.push("share");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

fn cleanup_old_log_files() -> io::Result<usize> {
    let mut cleaned_count = 0;
    let one_day_ago = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() - 86400) // 86400 seconds = 1 day
        .unwrap_or(0);
    
    if cfg!(windows) {
        // Clean both OneDrive and regular paths on Windows
        let paths = vec![get_onedrive_logs_path(), get_regular_logs_path()];
        
        for log_dir in paths {
            if log_dir.exists() {
                let entries = fs::read_dir(&log_dir)?;
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if filename.starts_with("nwclientLog") && filename.ends_with(".txt") {
                                if let Ok(metadata) = path.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                                            if duration.as_secs() < one_day_ago {
                                                match fs::remove_file(&path) {
                                                    Ok(_) => {
                                                        println!("Deleted old log file: {:?}", path);
                                                        cleaned_count += 1;
                                                    }
                                                    Err(e) => {
                                                        println!("Failed to delete {:?}: {}", path, e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Clean Unix path
        let log_dir = get_unix_logs_path();
        if log_dir.exists() {
            let entries = fs::read_dir(&log_dir)?;
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                        if filename.starts_with("nwclientLog") && filename.ends_with(".txt") {
                            if let Ok(metadata) = path.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                                        if duration.as_secs() < one_day_ago {
                                            match fs::remove_file(&path) {
                                                Ok(_) => {
                                                    println!("Deleted old log file: {:?}", path);
                                                    cleaned_count += 1;
                                                }
                                                Err(e) => {
                                                    println!("Failed to delete {:?}: {}", path, e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(cleaned_count)
}

/// Processes the entire log file to set up historical encounters
fn process_parsed_line(
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

fn process_full_log_file(
    file_path: &Path,
    encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    current_encounter_id: Arc<Mutex<Option<u64>>>,
    encounter_counter: Arc<Mutex<u64>>
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
                &encounter_counter
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

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// This function runs in a background thread, watching and parsing the log file.
fn log_watcher_thread(
    encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    current_encounter_id: Arc<Mutex<Option<u64>>>,
    encounter_counter: Arc<Mutex<u64>>
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
                match process_full_log_file(&latest_log_path, encounters.clone(), current_encounter_id.clone(), encounter_counter.clone()) {
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
                            let mut reader = io::BufReader::new(file);
                            if reader.seek(SeekFrom::Start(last_read_position)).is_ok() {
                                // Read remaining bytes and convert to string
                                let mut buffer = Vec::new();
                                if io::Read::read_to_end(&mut reader, &mut buffer).is_ok() {
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
                                            &encounter_counter
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

fn main() -> Result<(), Box<dyn Error>> {
    // Set up the shared state for encounters
    let encounters = Arc::new(Mutex::new(HashMap::new()));
    let current_encounter_id = Arc::new(Mutex::new(None));
    let encounter_counter = Arc::new(Mutex::new(1));
    
    let encounters_clone = encounters.clone();
    let current_encounter_clone = current_encounter_id.clone();
    let counter_clone = encounter_counter.clone();

    // Spawn the background thread for log watching.
    thread::spawn(move || {
        log_watcher_thread(encounters_clone, current_encounter_clone, counter_clone);
    });

    // Configure the native window options for a borderless, custom GUI.
    let native_options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])
            .with_min_inner_size([300.0, 200.0])
            .with_max_inner_size([1600.0, 1200.0]) // Set reasonable max size
            .with_resizable(true)
            .with_decorations(false) // Remove window decorations
            .with_always_on_top(), // Keep window always on top
        ..Default::default()
    };
    
    // Create the application state.
    let mut app = NwnLogApp::new();
    app.encounters = encounters;
    app.current_encounter_id = current_encounter_id;
    app.encounter_counter = encounter_counter;
    
    eframe::run_native(
        "NWN Log Overlay",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concealment_regex() {
        let test_line = "Mini Canoon attacks -ANCIENT EVIL | KLAUTH- : *target concealed: 50%* : *hit*";
        let parsed = parse_log_line(&format!("[CHAT WINDOW TEXT] [Wed Jul 30 14:58:02] {}", test_line));
        assert!(parsed.is_some(), "Should parse concealment line");
        
        if let Some(ParsedLine::Attack { attacker, target, result, concealment, .. }) = parsed {
            assert_eq!(attacker, "Mini Canoon");
            assert_eq!(target, "-ANCIENT EVIL | KLAUTH-");
            assert_eq!(result, "hit");
            assert!(concealment);
        } else {
            panic!("Expected Attack variant");
        }
    }

    #[test]
    fn test_concealment_miss_regex() {
        let test_line = "Mini Canoon attacks -ANCIENT EVIL | KLAUTH- : *target concealed: 50%* : *miss*";
        let parsed = parse_log_line(&format!("[CHAT WINDOW TEXT] [Wed Jul 30 14:58:02] {}", test_line));
        assert!(parsed.is_some(), "Should parse concealment miss line");
        
        if let Some(ParsedLine::Attack { attacker, target, result, concealment, .. }) = parsed {
            assert_eq!(attacker, "Mini Canoon");
            assert_eq!(target, "-ANCIENT EVIL | KLAUTH-");
            assert_eq!(result, "miss");
            assert!(concealment);
        } else {
            panic!("Expected Attack variant");
        }
    }

    #[test]
    fn test_normal_attack_regex() {
        let test_line = "Mini Canoon attacks -ANCIENT EVIL | KLAUTH- : *miss*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match normal attack line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Mini Canoon");
        assert_eq!(caps.name("target").unwrap().as_str(), "-ANCIENT EVIL | KLAUTH-");
        assert_eq!(caps.name("result").unwrap().as_str(), "miss");
    }

    #[test]
    fn test_expertise_death_attack_regex() {
        let test_line = "Expertise : Death Attack : Eviscera attacks Epic Disciple Of Mephisto : *hit*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match expertise death attack line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Eviscera");
        assert_eq!(caps.name("target").unwrap().as_str(), "Epic Disciple Of Mephisto");
        assert_eq!(caps.name("result").unwrap().as_str(), "hit");
    }

    #[test]
    fn test_aoo_expertise_death_attack_regex() {
        let test_line = "Attack Of Opportunity : Expertise : Death Attack : Eviscera attacks Draconic Warrior : *miss*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match AoO + Expertise + Death Attack line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Eviscera");
        assert_eq!(caps.name("target").unwrap().as_str(), "Draconic Warrior");
        assert_eq!(caps.name("result").unwrap().as_str(), "miss");
    }

    #[test]
    fn test_improved_expertise_regex() {
        let test_line = "Improved Expertise : grass cutter X attacks Someone : *miss*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Improved Expertise line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "grass cutter X");
        assert_eq!(caps.name("target").unwrap().as_str(), "Someone");
        assert_eq!(caps.name("result").unwrap().as_str(), "miss");
    }

    #[test]
    fn test_sneak_attack_death_attack_regex() {
        let test_line = "Sneak Attack + Death Attack : Funnelweb attacks AJATAR - GUARDIAN OF HELL : *hit*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Sneak Attack + Death Attack line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Funnelweb");
        assert_eq!(caps.name("target").unwrap().as_str(), "AJATAR - GUARDIAN OF HELL");
        assert_eq!(caps.name("result").unwrap().as_str(), "hit");
    }

    #[test]
    fn test_attack_of_opportunity_regex() {
        let test_line = "Attack Of Opportunity : Lucky Has Risen attacks AJATAR - GUARDIAN OF HELL : *miss*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Attack Of Opportunity line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Lucky Has Risen");
        assert_eq!(caps.name("target").unwrap().as_str(), "AJATAR - GUARDIAN OF HELL");
        assert_eq!(caps.name("result").unwrap().as_str(), "miss");
    }

    #[test]
    fn test_off_hand_attack_of_opportunity_regex() {
        let test_line = "Off Hand : Attack Of Opportunity : Din Din Din attacks AJATAR - GUARDIAN OF HELL : *miss*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Off Hand + Attack Of Opportunity line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Din Din Din");
        assert_eq!(caps.name("target").unwrap().as_str(), "AJATAR - GUARDIAN OF HELL");
        assert_eq!(caps.name("result").unwrap().as_str(), "miss");
    }

    #[test]
    fn test_off_hand_regex() {
        let test_line = "Off Hand : Eviscera attacks AJATAR - GUARDIAN OF HELL : *hit*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Off Hand line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Eviscera");
        assert_eq!(caps.name("target").unwrap().as_str(), "AJATAR - GUARDIAN OF HELL");
        assert_eq!(caps.name("result").unwrap().as_str(), "hit");
    }

    #[test]
    fn test_death_attack_regex() {
        let test_line = "Death Attack : Domino attacks AJATAR - GUARDIAN OF HELL : *hit*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Death Attack line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Domino");
        assert_eq!(caps.name("target").unwrap().as_str(), "AJATAR - GUARDIAN OF HELL");
        assert_eq!(caps.name("result").unwrap().as_str(), "hit");
    }

    #[test]
    fn test_sneak_attack_regex() {
        let test_line = "Sneak Attack : Din Din Din attacks Ahriman's Rage : *critical hit*";
        let caps = RE_ATTACK.captures(test_line);
        assert!(caps.is_some(), "Regex should match Sneak Attack line");
        
        let caps = caps.unwrap();
        assert_eq!(caps.name("attacker").unwrap().as_str(), "Din Din Din");
        assert_eq!(caps.name("target").unwrap().as_str(), "Ahriman's Rage");
        assert_eq!(caps.name("result").unwrap().as_str(), "critical hit");
    }
}
