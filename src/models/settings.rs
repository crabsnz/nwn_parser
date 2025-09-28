use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Caster level for spell calculations (1-40)
    pub caster_level: i32,
    /// Charisma modifier for spell calculations (-10 to +50)
    pub charisma_modifier: i32,
    /// Whether Extended Divine Might is active
    pub extended_divine_might: bool,
    /// Whether Extended Divine Shield is active
    pub extended_divine_shield: bool,
    /// Warning time for expiring buffs in seconds (1-30)
    pub buff_warning_seconds: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            caster_level: 1,
            charisma_modifier: 0,
            extended_divine_might: false,
            extended_divine_shield: false,
            buff_warning_seconds: 10,
        }
    }
}

impl AppSettings {
    /// Clamps caster level to valid range (1-40)
    pub fn set_caster_level(&mut self, level: i32) {
        self.caster_level = level.clamp(1, 40);
    }

    /// Clamps charisma modifier to reasonable range (-10 to +50)
    pub fn set_charisma_modifier(&mut self, modifier: i32) {
        self.charisma_modifier = modifier.clamp(-10, 50);
    }

    /// Clamps buff warning seconds to valid range (1-30)
    pub fn set_buff_warning_seconds(&mut self, seconds: u32) {
        self.buff_warning_seconds = seconds.clamp(1, 30);
    }
}