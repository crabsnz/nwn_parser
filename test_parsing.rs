use std::fs;

// Import the necessary modules
mod models;
mod parsing;
mod utils;

use models::PlayerRegistry;
use parsing::{parse_log_line, ParsedLine};

fn main() {
    let content = fs::read_to_string("test_player_detection.txt").expect("Could not read test file");
    let mut player_registry = PlayerRegistry::new();

    println!("Testing player identification parsing:");
    println!("=====================================");

    for line in content.lines() {
        if let Some(parsed) = parse_log_line(&line) {
            match &parsed {
                ParsedLine::PlayerJoin { account_name, .. } => {
                    player_registry.add_player_join(account_name.clone());
                    println!("✓ Detected main player account: {}", account_name);
                }
                ParsedLine::PlayerChat { account_name, character_name, .. } => {
                    player_registry.add_character_name(account_name.clone(), character_name.clone());
                    println!("✓ Associated character '{}' with account '{}'", character_name, account_name);
                }
                ParsedLine::PartyChat { character_name, .. } => {
                    player_registry.add_party_member(character_name.clone());
                    println!("✓ Detected player from party chat: {}", character_name);
                }
                ParsedLine::PartyJoin { character_name, .. } => {
                    player_registry.add_party_member(character_name.clone());
                    println!("✓ Detected player from party join: {}", character_name);
                }
                _ => {} // Ignore other lines for this test
            }
        }
    }

    println!("\nPlayer Registry Summary:");
    println!("========================");
    println!("Main player account: {:?}", player_registry.main_player_account);
    println!("Known players: {:?}", player_registry.players);
    println!("Character to account mapping: {:?}", player_registry.character_to_account);

    println!("\nTesting player identification:");
    println!("==============================");
    let test_names = ["willa", "Pink Sunsetalluren", "Athena", "Dummy", "Monster", "Nordock Avatar"];
    for name in test_names {
        let is_player = player_registry.is_player(name);
        println!("{}: {} ({})", name, if is_player { "Player" } else { "NPC/Monster" }, if is_player { "GREEN" } else { "RED" });
    }
}