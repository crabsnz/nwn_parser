use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::models::{Encounter, CombatantStats, ViewMode};
use crate::gui::helpers::compute_stats_hash;

pub struct NwnLogApp {
    /// All encounters, indexed by encounter ID
    pub encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    /// The current encounter being tracked
    pub current_encounter_id: Arc<Mutex<Option<u64>>>,
    /// Selected encounters for display (supports multiple selection)
    pub selected_encounter_ids: HashSet<u64>,
    /// Current view mode: individual encounters or combined view
    pub view_mode: ViewMode,
    /// Track if we're in resize mode
    pub is_resizing: bool,
    /// Minimum window size
    pub min_size: egui::Vec2,
    /// Text scaling factor
    pub text_scale: f32,
    /// Encounter counter
    pub encounter_counter: Arc<Mutex<u64>>,
    /// Cached sorted combatants to avoid re-sorting every frame
    pub cached_sorted_combatants: Vec<(String, CombatantStats)>,
    /// Hash of the current data to detect changes
    pub last_data_hash: u64,
}

impl NwnLogApp {
    pub fn new() -> Self {
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

    pub fn get_current_stats(&self) -> HashMap<String, CombatantStats> {
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
                                // Always show the current encounter data
                                return encounter.stats.clone();
                            }
                        }
                    }
                }
                
                // No current encounter, return empty stats
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

    pub fn get_combined_selected_stats(&self) -> HashMap<String, CombatantStats> {
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
                    
                    self.aggregate_stats(combined, stats);
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

    pub fn get_overall_stats(&self) -> HashMap<String, CombatantStats> {
        let encounters = self.encounters.lock().unwrap();
        self.combine_all_encounters_stats(&encounters)
    }

    fn combine_all_encounters_stats(&self, encounters: &HashMap<u64, Encounter>) -> HashMap<String, CombatantStats> {
        let mut overall_stats = HashMap::new();
        
        for encounter in encounters.values() {
            for (name, stats) in &encounter.stats {
                let overall = overall_stats.entry(name.clone()).or_insert_with(CombatantStats::default);
                
                self.aggregate_stats(overall, stats);
            }
        }
        
        overall_stats
    }
    
    fn aggregate_stats(&self, target: &mut CombatantStats, source: &CombatantStats) {
        // Aggregate all stats
        target.hits += source.hits;
        target.misses += source.misses;
        target.critical_hits += source.critical_hits;
        target.weapon_buffs += source.weapon_buffs;
        target.total_damage_dealt += source.total_damage_dealt;
        target.hit_damage += source.hit_damage;
        target.crit_damage += source.crit_damage;
        target.weapon_buff_damage += source.weapon_buff_damage;
        target.times_attacked += source.times_attacked;
        target.total_damage_received += source.total_damage_received;
        target.total_damage_absorbed += source.total_damage_absorbed;
        
        // Aggregate damage by type dealt
        for (dtype, amount) in &source.damage_by_type_dealt {
            *target.damage_by_type_dealt.entry(dtype.clone()).or_default() += *amount;
        }
        
        // Aggregate hit damage by type
        for (dtype, amount) in &source.hit_damage_by_type {
            *target.hit_damage_by_type.entry(dtype.clone()).or_default() += *amount;
        }
        
        // Aggregate crit damage by type
        for (dtype, amount) in &source.crit_damage_by_type {
            *target.crit_damage_by_type.entry(dtype.clone()).or_default() += *amount;
        }
        
        // Aggregate weapon buff damage by type
        for (dtype, amount) in &source.weapon_buff_damage_by_type {
            *target.weapon_buff_damage_by_type.entry(dtype.clone()).or_default() += *amount;
        }
        
        // Aggregate damage sources
        for (source_name, amount) in &source.damage_by_source_dealt {
            *target.damage_by_source_dealt.entry(source_name.clone()).or_default() += *amount;
        }
        
        // Aggregate damage by source and type dealt
        for (source_name, type_map) in &source.damage_by_source_and_type_dealt {
            let target_type_map = target.damage_by_source_and_type_dealt.entry(source_name.clone()).or_default();
            for (dtype, amount) in type_map {
                *target_type_map.entry(dtype.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate damage by target
        for (target_name, amount) in &source.damage_by_target_dealt {
            *target.damage_by_target_dealt.entry(target_name.clone()).or_default() += *amount;
        }
        
        // Aggregate damage by target and source dealt
        for (target_name, source_map) in &source.damage_by_target_and_source_dealt {
            let target_map = target.damage_by_target_and_source_dealt.entry(target_name.clone()).or_default();
            for (source_name, amount) in source_map {
                *target_map.entry(source_name.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate damage by target, source, and type dealt
        for (target_name, source_map) in &source.damage_by_target_source_and_type_dealt {
            let target_map = target.damage_by_target_source_and_type_dealt.entry(target_name.clone()).or_default();
            for (source_name, type_map) in source_map {
                let source_map = target_map.entry(source_name.clone()).or_default();
                for (dtype, amount) in type_map {
                    *source_map.entry(dtype.clone()).or_default() += *amount;
                }
            }
        }
        
        // Aggregate hit damage by target and type
        for (target_name, type_map) in &source.hit_damage_by_target_type {
            let target_map = target.hit_damage_by_target_type.entry(target_name.clone()).or_default();
            for (dtype, amount) in type_map {
                *target_map.entry(dtype.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate crit damage by target and type
        for (target_name, type_map) in &source.crit_damage_by_target_type {
            let target_map = target.crit_damage_by_target_type.entry(target_name.clone()).or_default();
            for (dtype, amount) in type_map {
                *target_map.entry(dtype.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate weapon buff damage by target and type
        for (target_name, type_map) in &source.weapon_buff_damage_by_target_type {
            let target_map = target.weapon_buff_damage_by_target_type.entry(target_name.clone()).or_default();
            for (dtype, amount) in type_map {
                *target_map.entry(dtype.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate damage sources received
        for (source_name, amount) in &source.damage_by_source_received {
            *target.damage_by_source_received.entry(source_name.clone()).or_default() += *amount;
        }
        
        // Aggregate damage by source and type received
        for (source_name, type_map) in &source.damage_by_source_and_type_received {
            let target_source_map = target.damage_by_source_and_type_received.entry(source_name.clone()).or_default();
            for (dtype, amount) in type_map {
                *target_source_map.entry(dtype.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate damage by attacker received
        for (attacker, amount) in &source.damage_by_attacker_received {
            *target.damage_by_attacker_received.entry(attacker.clone()).or_default() += *amount;
        }
        
        // Aggregate damage by attacker and source received
        for (attacker, source_map) in &source.damage_by_attacker_and_source_received {
            let target_attacker_map = target.damage_by_attacker_and_source_received.entry(attacker.clone()).or_default();
            for (source_name, amount) in source_map {
                *target_attacker_map.entry(source_name.clone()).or_default() += *amount;
            }
        }
        
        // Aggregate absorbed damage by type
        for (dtype, amount) in &source.absorbed_by_type {
            *target.absorbed_by_type.entry(dtype.clone()).or_default() += *amount;
        }
        
        // Update timing for combined stats
        if let Some(first) = source.first_action_time {
            target.first_action_time = Some(
                target.first_action_time.map_or(first, |existing| existing.min(first))
            );
        }
        if let Some(last) = source.last_action_time {
            target.last_action_time = Some(
                target.last_action_time.map_or(last, |existing| existing.max(last))
            );
        }
    }
    
    pub fn update_sorted_cache(&mut self, stats_map: &HashMap<String, CombatantStats>) {
        let current_hash = compute_stats_hash(stats_map);
        
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
}