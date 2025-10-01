use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct CombatantStats {
    // --- Stats for actions performed by the combatant ---
    pub hits: u32,
    pub misses: u32,
    pub critical_hits: u32,
    pub concealment_dodges: u32,
    pub weapon_buffs: u32,
    pub total_damage_dealt: u32,
    pub hit_damage: u32,
    pub crit_damage: u32,
    pub weapon_buff_damage: u32,
    pub damage_by_type_dealt: HashMap<String, u32>,
    pub hit_damage_by_type: HashMap<String, u32>,
    pub crit_damage_by_type: HashMap<String, u32>,
    pub weapon_buff_damage_by_type: HashMap<String, u32>,
    pub damage_by_source_dealt: HashMap<String, u32>, // "Attack", "Spell: Fireball", etc.
    pub damage_by_source_and_type_dealt: HashMap<String, HashMap<String, u32>>, // Source -> Type -> Amount
    pub damage_by_target_dealt: HashMap<String, u32>, // Target -> Total damage to that target
    pub damage_by_target_and_source_dealt: HashMap<String, HashMap<String, u32>>, // Target -> Source -> Amount
    pub damage_by_target_source_and_type_dealt: HashMap<String, HashMap<String, HashMap<String, u32>>>, // Target -> Source -> Type -> Amount
    pub hit_damage_by_target_type: HashMap<String, HashMap<String, u32>>, // Target -> Type -> Amount (for hit damage only)
    pub crit_damage_by_target_type: HashMap<String, HashMap<String, u32>>, // Target -> Type -> Amount (for crit damage only)
    pub weapon_buff_damage_by_target_type: HashMap<String, HashMap<String, u32>>, // Target -> Type -> Amount (for weapon buff damage only)

    // --- Stats for actions received by the combatant ---
    pub times_attacked: u32,
    pub total_damage_received: u32,
    pub damage_by_type_received: HashMap<String, u32>,
    pub damage_by_source_received: HashMap<String, u32>, // Track who/what damaged this combatant
    pub damage_by_source_and_type_received: HashMap<String, HashMap<String, u32>>, // Source -> Type -> Amount
    pub damage_by_attacker_received: HashMap<String, u32>, // Attacker -> Total damage from that attacker
    pub damage_by_attacker_and_source_received: HashMap<String, HashMap<String, u32>>, // Attacker -> Source -> Amount

    // --- Special stats like absorption ---
    pub total_damage_absorbed: u32,
    pub absorbed_by_type: HashMap<String, u32>,
    
    // --- Timing for DPS calculation ---
    pub first_action_time: Option<u64>,
    pub last_action_time: Option<u64>,
}

impl CombatantStats {
    pub fn calculate_dps(&self) -> Option<f64> {
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

    pub fn calculate_dtps(&self) -> Option<f64> {
        if let (Some(first), Some(last)) = (self.first_action_time, self.last_action_time) {
            let duration_secs = if last > first { last - first } else { 1 };
            if duration_secs > 0 && self.total_damage_received > 0 {
                Some(self.total_damage_received as f64 / duration_secs as f64)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn update_action_time(&mut self, timestamp: u64) {
        if self.first_action_time.is_none() {
            self.first_action_time = Some(timestamp);
        }
        self.last_action_time = Some(timestamp);
    }
}