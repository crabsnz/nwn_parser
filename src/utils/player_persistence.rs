use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use crate::models::PlayerRegistry;

const PLAYERS_FILE: &str = "players.json";

pub fn get_players_file_path() -> PathBuf {
    PathBuf::from(PLAYERS_FILE)
}

pub fn load_player_registry() -> PlayerRegistry {
    let file_path = get_players_file_path();

    if !file_path.exists() {
        println!("No existing player registry found, starting with empty registry");
        return PlayerRegistry::new();
    }

    match fs::read_to_string(&file_path) {
        Ok(content) => {
            match serde_json::from_str::<PlayerRegistry>(&content) {
                Ok(mut registry) => {
                    println!("Loaded player registry with {} players", registry.players.len());
                    registry.cleanup_temporary_accounts();
                    // Save the cleaned up registry
                    let _ = save_player_registry(&registry);
                    registry
                }
                Err(e) => {
                    eprintln!("Error parsing player registry JSON: {}. Starting with empty registry.", e);
                    PlayerRegistry::new()
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading player registry file: {}. Starting with empty registry.", e);
            PlayerRegistry::new()
        }
    }
}

pub fn save_player_registry(registry: &PlayerRegistry) -> io::Result<()> {
    let file_path = get_players_file_path();

    let json_content = serde_json::to_string_pretty(registry)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON serialization error: {}", e)))?;

    let mut file = fs::File::create(&file_path)?;
    file.write_all(json_content.as_bytes())?;
    file.flush()?;

    println!("Saved player registry with {} players", registry.players.len());
    Ok(())
}

pub fn auto_save_player_registry(registry: &PlayerRegistry) {
    if let Err(e) = save_player_registry(registry) {
        eprintln!("Failed to auto-save player registry: {}", e);
    }
}