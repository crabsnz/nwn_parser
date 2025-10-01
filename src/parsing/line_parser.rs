use std::collections::HashMap;
use crate::parsing::regex::*;
use crate::utils::time::{parse_timestamp, get_current_timestamp};

#[derive(Debug)]
pub enum ParsedLine {
    Attack { attacker: String, target: String, result: String, concealment: bool, timestamp: u64 },
    Damage { attacker: String, target: String, total: u32, breakdown: HashMap<String, u32>, timestamp: u64 },
    Absorb { target: String, amount: u32, dtype: String, timestamp: u64 },
    AbsorbResistance { target: String, amount: u32, timestamp: u64 },
    AbsorbReduction { target: String, amount: u32, timestamp: u64 },
    SpellResist { target: String, spell: String, result: String, timestamp: u64 },
    Save { target: String, save_type: String, element: String, result: String, timestamp: u64 },
    Casting { caster: String, spell: String, timestamp: u64 },
    Casts { caster: String, spell: String, timestamp: u64 },
    PlayerJoin { account_name: String, timestamp: u64 },
    PlayerChat { account_name: String, character_name: String, chat_type: String, timestamp: u64 },
    PartyChat { character_name: String, timestamp: u64 },
    PartyJoin { character_name: String, timestamp: u64 },
    Resting { timestamp: u64 },
    BuffExpired { spell_name: String, timestamp: u64 },
}

pub fn is_long_duration_spell(spell: &str) -> bool {
    matches!(spell, 
        "Isaac's Greater Missile Storm" | 
        "Isaac's Lesser Missile Storm" | 
        "Magic Missile" | 
        "Flame Arrow" | 
        "Ball Lightning"
    )
}

pub fn get_spell_damage_type(spell: &str) -> Option<&'static str> {
    match spell {
        "Flame Arrow" => Some("Fire"),
        "Ball Lightning" => Some("Electrical"),
        "Isaac's Greater Missile Storm" | "Isaac's Lesser Missile Storm" | "Magic Missile" => Some("Magical"),
        _ => None,
    }
}

pub fn parse_log_line(line: &str) -> Option<ParsedLine> {
    let timestamp = if let Some(caps) = RE_TIMESTAMP.captures(line) {
        parse_timestamp(&caps[1])
    } else {
        get_current_timestamp() // Fallback to current time if no timestamp
    };

    let clean_line = line.trim().strip_prefix("[CHAT WINDOW TEXT]").and_then(|s| s.splitn(2, ']').nth(1)).unwrap_or(line).trim();

    // Check for rest detection first (high priority for buff clearing)
    if RE_RESTING.is_match(clean_line) {
        return Some(ParsedLine::Resting { timestamp });
    }

    // Check for buff expiration
    if let Some(caps) = RE_BUFF_EXPIRED.captures(clean_line) {
        return Some(ParsedLine::BuffExpired {
            spell_name: caps["spell_name"].trim().to_string(),
            timestamp,
        });
    }

    // Check for player identification patterns first (these have higher priority)
    if let Some(caps) = RE_PLAYER_JOIN.captures(clean_line) {
        return Some(ParsedLine::PlayerJoin {
            account_name: caps["account"].to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_PLAYER_CHAT.captures(clean_line) {
        return Some(ParsedLine::PlayerChat {
            account_name: caps["account"].to_string(),
            character_name: caps["character"].trim().to_string(),
            chat_type: caps["chat_type"].to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_PARTY_CHAT.captures(clean_line) {
        return Some(ParsedLine::PartyChat {
            character_name: caps["character"].trim().to_string(),
            timestamp,
        });
    }

    if let Some(caps) = RE_PARTY_JOIN.captures(clean_line) {
        return Some(ParsedLine::PartyJoin {
            character_name: caps["character"].trim().to_string(),
            timestamp,
        });
    }

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

    if let Some(caps) = RE_ABSORB_RESISTANCE.captures(clean_line) {
        return Some(ParsedLine::AbsorbResistance {
            target: caps["target"].trim().to_string(),
            amount: caps["amount"].parse().unwrap_or(0),
            timestamp,
        });
    }

    if let Some(caps) = RE_ABSORB_REDUCTION.captures(clean_line) {
        return Some(ParsedLine::AbsorbReduction {
            target: caps["target"].trim().to_string(),
            amount: caps["amount"].parse().unwrap_or(0),
            timestamp,
        });
    }

    None
}