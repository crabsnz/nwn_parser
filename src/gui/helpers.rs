use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use crate::models::CombatantStats;

pub fn compute_stats_hash(stats_map: &HashMap<String, CombatantStats>) -> u64 {
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