use std::collections::HashMap;
use crate::models::stats::CombatantStats;
use crate::utils::time::format_duration;

#[derive(Debug, Clone)]
pub struct Encounter {
    pub id: u64,
    pub start_time: u64,
    pub end_time: u64,
    pub stats: HashMap<String, CombatantStats>,
    pub most_damaged_participant: String,
    pub total_damage: u32,
}

impl Encounter {
    pub fn new(id: u64, start_time: u64) -> Self {
        Self {
            id,
            start_time,
            end_time: start_time,
            stats: HashMap::new(),
            most_damaged_participant: String::new(),
            total_damage: 0,
        }
    }

    pub fn update_most_damaged(&mut self) {
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

    pub fn duration(&self) -> u64 {
        if self.end_time >= self.start_time {
            self.end_time - self.start_time
        } else {
            0
        }
    }

    pub fn get_display_name(&self) -> String {
        let duration_str = format_duration(self.duration());
        if self.most_damaged_participant.is_empty() {
            format!("#{} {} Fight", self.id, duration_str)
        } else {
            format!("#{} {} {}", self.id, duration_str, self.most_damaged_participant)
        }
    }
}