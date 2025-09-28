use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerData {
    pub account_name: String,
    pub character_names: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PlayerRegistry {
    pub players: HashMap<String, PlayerData>, // account_name -> PlayerData
    pub character_to_account: HashMap<String, String>, // character_name -> account_name
    pub main_player_account: Option<String>, // The main player (first "has joined as a player")
}

impl PlayerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_player_join(&mut self, account_name: String) {
        // Set as main player if this is the first time we see "has joined as a player"
        if self.main_player_account.is_none() {
            self.main_player_account = Some(account_name.clone());
        }

        // Add player if not already exists
        if !self.players.contains_key(&account_name) {
            self.players.insert(account_name.clone(), PlayerData {
                account_name: account_name.clone(),
                character_names: Vec::new(),
            });
        }
    }

    pub fn add_character_name(&mut self, account_name: String, character_name: String) {
        // Ensure player exists
        if !self.players.contains_key(&account_name) {
            self.players.insert(account_name.clone(), PlayerData {
                account_name: account_name.clone(),
                character_names: Vec::new(),
            });
        }

        // Add character name if not already associated with this account
        if let Some(player) = self.players.get_mut(&account_name) {
            if !player.character_names.contains(&character_name) {
                player.character_names.push(character_name.clone());
            }
        }

        // Update reverse mapping
        self.character_to_account.insert(character_name, account_name);
    }

    pub fn add_party_member(&mut self, character_name: String) {
        // If we don't already know this character, mark them as a player
        if !self.character_to_account.contains_key(&character_name) {
            // Use character name as temporary account name for party members
            // This will be updated if we later see them in chat with their real account
            let temp_account = format!("player_{}", character_name);
            self.add_character_name(temp_account, character_name);
        }
    }

    pub fn is_player(&self, name: &str) -> bool {
        // Check if this is a known character name
        self.character_to_account.contains_key(name)
    }

    pub fn get_main_player_info(&self) -> Option<(String, String)> {
        if let Some(account) = &self.main_player_account {
            if let Some(player) = self.players.get(account) {
                if let Some(character) = player.character_names.first() {
                    return Some((account.clone(), character.clone()));
                }
            }
        }
        None
    }

    pub fn get_display_name(&self, character_name: &str) -> String {
        if let Some(account) = self.character_to_account.get(character_name) {
            format!("[{}] {}", account, character_name)
        } else {
            character_name.to_string()
        }
    }

    pub fn clear_character_names(&mut self, account_name: &str) {
        if let Some(player) = self.players.get_mut(account_name) {
            // Remove all character->account mappings for this account
            for character_name in &player.character_names {
                self.character_to_account.remove(character_name);
            }
            // Clear the character names list
            player.character_names.clear();
        }
    }
}