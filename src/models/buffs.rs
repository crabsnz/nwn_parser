use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::models::AppSettings;
use crate::utils::get_current_timestamp;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBuff {
    pub name: String,
    pub caster: String,
    pub start_time: u64,
    pub duration_seconds: u64,
}

impl ActiveBuff {
    pub fn new(name: String, caster: String, duration_seconds: u64) -> Self {
        Self {
            name,
            caster,
            start_time: get_current_timestamp(),
            duration_seconds,
        }
    }

    pub fn remaining_seconds(&self) -> i64 {
        let current_time = get_current_timestamp();
        let elapsed = current_time.saturating_sub(self.start_time);
        (self.duration_seconds as i64) - (elapsed as i64)
    }

    pub fn is_expired(&self) -> bool {
        self.remaining_seconds() <= 0
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BuffTracker {
    pub active_buffs: HashMap<String, ActiveBuff>, // buff_name -> ActiveBuff
}

impl BuffTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_buff(&mut self, name: String, caster: String, settings: &AppSettings) {
        if let Some(duration) = Self::calculate_buff_duration(&name, settings) {
            let buff = ActiveBuff::new(name.clone(), caster, duration);
            self.active_buffs.insert(name, buff);
        }
    }

    pub fn is_trackable_buff(spell_name: &str, settings: &AppSettings) -> bool {
        Self::calculate_buff_duration(spell_name, settings).is_some()
    }

    pub fn remove_expired_buffs(&mut self) {
        self.active_buffs.retain(|_, buff| !buff.is_expired());
    }

    pub fn clear_all_buffs(&mut self) {
        self.active_buffs.clear();
    }

    pub fn get_active_buffs(&self) -> Vec<&ActiveBuff> {
        self.active_buffs
            .values()
            .filter(|buff| !buff.is_expired())
            .collect()
    }

    fn calculate_buff_duration(spell_name: &str, settings: &AppSettings) -> Option<u64> {
        let duration = match spell_name {
            "Divine Favor" => Some(120), // Flat 2 turns
            "Divine Might" => {
                // Ensure minimum duration even with negative charisma modifier
                let base_duration = (settings.charisma_modifier.max(-10) as i64 * 2 * 6).max(10) as u64;
                if settings.extended_divine_might {
                    Some(base_duration + 10 * 6) // Extended item adds 10 rounds
                } else {
                    Some(base_duration)
                }
            },
            "Divine Shield" => {
                // Ensure minimum duration even with negative charisma modifier
                let base_duration = (settings.charisma_modifier.max(-10) as i64 * 2 * 6).max(10) as u64;
                if settings.extended_divine_shield {
                    Some(base_duration + 10 * 6) // Extended item adds 10 rounds
                } else {
                    Some(base_duration)
                }
            },
            "Divine Power" => {
                // Divine Power: 6 round per caster level * 2 (assume extended), with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 * 2)
            },
            "Tenser's Transformation" => {
                // Tenser's Transformation: 1 round per caster level * 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 * 2)
            },
            "Greater Sanctuary" => Some(40), // Flat 2 rounds * 2 (assume extended)
            "Bigby's Interposing Hand" => {
                // Bigby's Interposing Hand: 2 rounds + (1 round per caster level / 5) * 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some((2 + caster_level / 2 * 6) * 2)
            },
            "Acid Fog" => {
                // Acid Fog: 1 round per caster level / 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 / 2)
            },
            "Cloudkill" => {
                // Cloudkill: 1 round per caster level / 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 / 2)
            },
            "Mestil's Acid Sheath" => {
                // Mestil's Acid Sheath: 1 round per caster level * 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 * 2)
            },
            "Elemental Shield" => {
                // Elemental Shield: 1 round per caster level * 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 * 2)
            },
            "Death Armor" => {
                // Death Armor: 1 round per caster level * 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 * 2)
            },
            "Blade Thirst" => {
                // Blade Thirst 1 round per caster level * 2, with minimum caster level of 1
                let caster_level = settings.caster_level.max(1) as u64;
                Some(caster_level * 6 * 2)
            },
            _ => None, // Unknown spells are not tracked
        };

        if let Some(d) = duration {
            println!("Buff Duration Calculation: {} - caster_level: {}, cha_mod: {}, duration: {}s",
                     spell_name, settings.caster_level, settings.charisma_modifier, d);
        }
        duration
    }
}