use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::models::{Encounter, CombatantStats, ViewMode, PlayerRegistry, AppSettings, BuffTracker, DamageViewMode, CombatantFilter};
use crate::gui::helpers::compute_stats_hash;
use crate::gui::logs_window::LogsWindowState;
use crate::utils::{load_player_registry, load_app_settings};

pub struct NwnLogApp {
    /// All encounters, indexed by encounter ID
    pub encounters: Arc<Mutex<HashMap<u64, Encounter>>>,
    /// The current encounter being tracked
    pub current_encounter_id: Arc<Mutex<Option<u64>>>,
    /// Selected encounters for display (supports multiple selection)
    pub selected_encounter_ids: HashSet<u64>,
    /// Current view mode: individual encounters or combined view
    pub view_mode: ViewMode,
    /// Text scaling factor
    pub text_scale: f32,
    /// Encounter counter
    pub encounter_counter: Arc<Mutex<u64>>,
    /// Cached sorted combatants to avoid re-sorting every frame
    pub cached_sorted_combatants: Vec<(String, CombatantStats)>,
    /// Hash of the current data to detect changes
    pub last_data_hash: u64,
    /// Player registry for tracking known players
    pub player_registry: Arc<Mutex<PlayerRegistry>>,
    /// Whether to show the options window
    pub show_options: bool,
    /// Buff tracker for divine spells
    pub buff_tracker: Arc<Mutex<BuffTracker>>,
    /// Whether the buff window has been spawned
    pub buff_window_spawned: bool,
    /// Shared settings for background thread access
    pub settings_ref: Option<Arc<Mutex<AppSettings>>>,
    /// Current damage view mode (done/taken)
    pub damage_view_mode: DamageViewMode,
    /// Current combatant filter (all/friendlies/enemies)
    pub combatant_filter: CombatantFilter,
    /// Open player detail windows
    pub open_detail_windows: HashMap<String, bool>,
    /// Last damage view mode to detect changes
    pub last_damage_view_mode: DamageViewMode,
    /// Last combatant filter to detect changes
    pub last_combatant_filter: CombatantFilter,
    /// Whether the first two button rows are minimized
    pub rows_minimized: bool,
    /// Pending log directory change (before confirmation)
    pub pending_log_directory: Option<String>,
    /// Whether to show confirmation for log directory change
    pub show_log_dir_confirm: bool,
    /// Signal to reload logs from new directory
    pub log_reload_requested: Arc<Mutex<bool>>,
    /// Logs window state
    pub logs_window_state: LogsWindowState,
    /// Whether the logs window is open
    pub logs_window_open: bool,
    /// Whether the encounters popup is open
    pub encounters_popup_open: bool,
    /// Rect of the encounters button for popup positioning
    pub encounters_button_rect: Option<egui::Rect>,
}

impl NwnLogApp {
    pub fn new() -> Self {
        // Load player registry from file
        let player_registry = load_player_registry();
        // Load app settings from file
        let settings = load_app_settings();

        Self {
            encounters: Arc::new(Mutex::new(HashMap::new())),
            current_encounter_id: Arc::new(Mutex::new(None)),
            selected_encounter_ids: HashSet::new(),
            view_mode: ViewMode::CurrentFight,
            text_scale: 1.0,
            encounter_counter: Arc::new(Mutex::new(1)),
            cached_sorted_combatants: Vec::new(),
            last_data_hash: 0,
            player_registry: Arc::new(Mutex::new(player_registry)),
            show_options: false,
            buff_tracker: Arc::new(Mutex::new(BuffTracker::new())),
            buff_window_spawned: false,
            settings_ref: Some(Arc::new(Mutex::new(settings))),
            damage_view_mode: DamageViewMode::default(),
            combatant_filter: CombatantFilter::default(),
            open_detail_windows: HashMap::new(),
            last_damage_view_mode: DamageViewMode::default(),
            last_combatant_filter: CombatantFilter::default(),
            rows_minimized: false,
            pending_log_directory: None,
            show_log_dir_confirm: false,
            log_reload_requested: Arc::new(Mutex::new(false)),
            logs_window_state: LogsWindowState::default(),
            logs_window_open: false,
            encounters_popup_open: false,
            encounters_button_rect: None,
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

        // Check if data, filter, or view mode changed
        let filter_changed = self.combatant_filter != self.last_combatant_filter;
        let view_mode_changed = self.damage_view_mode != self.last_damage_view_mode;
        let data_changed = current_hash != self.last_data_hash;

        // Only re-sort if something has changed
        if data_changed || filter_changed || view_mode_changed {
            // Apply combatant filter
            let filtered_stats: HashMap<String, CombatantStats> = stats_map.iter()
                .filter(|(name, _stats)| {
                    match self.combatant_filter {
                        crate::models::CombatantFilter::All => true,
                        crate::models::CombatantFilter::Friendlies => {
                            // Check if this is a known player
                            if let Ok(registry) = self.player_registry.lock() {
                                registry.is_player(name)
                            } else {
                                false
                            }
                        },
                        crate::models::CombatantFilter::Enemies => {
                            // Check if this is NOT a known player
                            if let Ok(registry) = self.player_registry.lock() {
                                !registry.is_player(name)
                            } else {
                                true
                            }
                        }
                    }
                })
                .map(|(name, stats)| (name.clone(), stats.clone()))
                .collect();

            self.cached_sorted_combatants = filtered_stats.iter()
                .map(|(name, stats)| (name.clone(), stats.clone()))
                .collect();

            // Sort based on damage view mode
            match self.damage_view_mode {
                crate::models::DamageViewMode::DamageDone => {
                    self.cached_sorted_combatants.sort_by(|a, b| {
                        b.1.total_damage_dealt.cmp(&a.1.total_damage_dealt)
                            .then(b.1.total_damage_received.cmp(&a.1.total_damage_received))
                            .then(a.0.cmp(&b.0))
                    });
                },
                crate::models::DamageViewMode::DamageTaken => {
                    self.cached_sorted_combatants.sort_by(|a, b| {
                        b.1.total_damage_received.cmp(&a.1.total_damage_received)
                            .then(b.1.total_damage_dealt.cmp(&a.1.total_damage_dealt))
                            .then(a.0.cmp(&b.0))
                    });
                }
            }

            self.last_data_hash = current_hash;
            self.last_combatant_filter = self.combatant_filter.clone();
            self.last_damage_view_mode = self.damage_view_mode.clone();
        }
    }

    /// Format damage stats for copying to clipboard
    pub fn format_damage_for_copy(&self) -> String {
        use crate::models::DamageViewMode;

        let header = match self.damage_view_mode {
            DamageViewMode::DamageDone => " Damage Done ",
            DamageViewMode::DamageTaken => " Damage Taken ",
        };

        let mut lines = vec![header.to_string()];

        // Calculate total damage for percentage
        let total_damage: u32 = self.cached_sorted_combatants.iter()
            .map(|(_, s)| match self.damage_view_mode {
                DamageViewMode::DamageDone => s.total_damage_dealt,
                DamageViewMode::DamageTaken => s.total_damage_received,
            }).sum();

        // Find max damage value to determine alignment width
        let max_damage: u32 = self.cached_sorted_combatants.iter()
            .map(|(_, s)| match self.damage_view_mode {
                DamageViewMode::DamageDone => s.total_damage_dealt,
                DamageViewMode::DamageTaken => s.total_damage_received,
            }).max().unwrap_or(0);

        let damage_width = max_damage.to_string().len().max(4); // At least 4 chars

        for (name, stats) in &self.cached_sorted_combatants {
            let damage = match self.damage_view_mode {
                DamageViewMode::DamageDone => stats.total_damage_dealt,
                DamageViewMode::DamageTaken => stats.total_damage_received,
            };

            let dps = stats.calculate_dps().map(|d| d.round() as u32).unwrap_or(0);
            let percentage = if total_damage > 0 && damage > 0 {
                (damage as f32 / total_damage as f32 * 100.0).round() as u32
            } else {
                0
            };

            // Format with right-aligned damage value
            lines.push(format!("{:<16} {:>width$} ({}, {}%)",
                name, damage, dps, percentage, width = damage_width));
        }

        lines.join("\n")
    }
}