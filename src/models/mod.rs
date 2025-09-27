pub mod stats;
pub mod encounter;
pub mod context;
pub mod player;
pub mod settings;
pub mod buffs;

pub use stats::CombatantStats;
pub use encounter::Encounter;
pub use context::{ViewMode, SpellContext, PendingAttack, PendingSpell, LongDurationSpell};

#[derive(Debug, Clone, PartialEq)]
pub enum DamageViewMode {
    DamageDone,
    DamageTaken,
}

impl Default for DamageViewMode {
    fn default() -> Self {
        DamageViewMode::DamageDone
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CombatantFilter {
    All,
    Friendlies,
    Enemies,
}

impl Default for CombatantFilter {
    fn default() -> Self {
        CombatantFilter::All
    }
}
pub use player::{PlayerData, PlayerRegistry};
pub use settings::AppSettings;
pub use buffs::{ActiveBuff, BuffTracker};