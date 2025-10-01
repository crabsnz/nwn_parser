use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use crate::models::AppSettings;
use crate::log::finder::get_default_log_directory;

const SETTINGS_FILE: &str = "settings.json";

pub fn get_settings_file_path() -> PathBuf {
    PathBuf::from(SETTINGS_FILE)
}

pub fn load_app_settings() -> AppSettings {
    let file_path = get_settings_file_path();

    let mut settings = if !file_path.exists() {
        println!("No existing settings found, using defaults");
        AppSettings::default()
    } else {
        match fs::read_to_string(&file_path) {
            Ok(content) => {
                match serde_json::from_str::<AppSettings>(&content) {
                    Ok(settings) => {
                        println!("Loaded settings: caster level {}, CHA mod {}, Divine Might: {}, Divine Shield: {}",
                                 settings.caster_level, settings.charisma_modifier,
                                 settings.extended_divine_might, settings.extended_divine_shield);
                        settings
                    }
                    Err(e) => {
                        eprintln!("Error parsing settings JSON: {}. Using defaults.", e);
                        AppSettings::default()
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading settings file: {}. Using defaults.", e);
                AppSettings::default()
            }
        }
    };

    // Auto-populate log directory if it's not set
    if settings.log_directory.is_none() {
        if let Some(default_dir) = get_default_log_directory() {
            settings.log_directory = Some(default_dir);
            // Save the updated settings with the auto-detected directory
            if let Err(e) = save_app_settings(&settings) {
                eprintln!("Failed to save auto-detected log directory: {}", e);
            }
        }
    }

    settings
}

pub fn save_app_settings(settings: &AppSettings) -> io::Result<()> {
    let file_path = get_settings_file_path();

    let json_content = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON serialization error: {}", e)))?;

    let mut file = fs::File::create(&file_path)?;
    file.write_all(json_content.as_bytes())?;
    file.flush()?;

    println!("Saved settings: caster level {}, CHA mod {}, Divine Might: {}, Divine Shield: {}",
             settings.caster_level, settings.charisma_modifier,
             settings.extended_divine_might, settings.extended_divine_shield);
    Ok(())
}

pub fn auto_save_app_settings(settings: &AppSettings) {
    if let Err(e) = save_app_settings(settings) {
        eprintln!("Failed to auto-save settings: {}", e);
    }
}