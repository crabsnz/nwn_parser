#[derive(Clone, Copy, PartialEq)]
pub enum ViewMode {
    CurrentFight,
    OverallStats,
    MultipleSelected,
}

#[derive(Debug, Clone)]
pub struct SpellContext {
    pub caster: String,
    pub spell: String,
    pub affected_targets: Vec<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct PendingAttack {
    pub attacker: String,
    pub target: String,
    pub timestamp: u64,
    pub is_crit: bool,
}

#[derive(Debug, Clone)]
pub struct PendingSpell {
    pub caster: String,
    pub target: String,
    pub spell: String,
    pub timestamp: u64,
    pub had_save_roll: bool,
    pub had_damage_immunity: bool,
}

#[derive(Debug, Clone)]
pub struct LongDurationSpell {
    pub caster: String,
    pub target: String,
    pub spell: String,
    pub timestamp: u64,
    pub had_save_roll: bool,
    pub had_damage_immunity: bool,
}