
use eframe::{egui, NativeOptions};
use egui::ViewportBuilder;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::thread;

// Module declarations
mod models;
mod parsing;
mod log;
mod gui;
mod utils;

// Re-exports for convenience
use gui::NwnLogApp;
use log::log_watcher_thread;
use models::PlayerRegistry;
use utils::load_player_registry;

fn main() -> Result<(), Box<dyn Error>> {
    // Set up the shared state for encounters
    let encounters = Arc::new(Mutex::new(HashMap::new()));
    let current_encounter_id = Arc::new(Mutex::new(None));
    let encounter_counter = Arc::new(Mutex::new(1));

    // Create the application state to get shared references
    let mut app = NwnLogApp::new();
    let player_registry = app.player_registry.clone();
    let buff_tracker = app.buff_tracker.clone();
    let settings = app.settings_ref.clone().unwrap();

    let encounters_clone = encounters.clone();
    let current_encounter_clone = current_encounter_id.clone();
    let counter_clone = encounter_counter.clone();
    let registry_clone = player_registry.clone();
    let buff_tracker_clone = buff_tracker.clone();
    let settings_clone = settings.clone();

    // Spawn the background thread for log watching.
    thread::spawn(move || {
        log_watcher_thread(encounters_clone, current_encounter_clone, counter_clone, registry_clone, buff_tracker_clone, settings_clone);
    });

    // Configure the native window options for a borderless, custom GUI.
    let native_options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])
            .with_min_inner_size([300.0, 200.0])
            .with_max_inner_size([1600.0, 1200.0]) // Set reasonable max size
            .with_resizable(true)
            .with_decorations(false) // Remove window decorations
            .with_always_on_top(), // Keep window always on top
        ..Default::default()
    };
    
    // Update the app with the shared state
    app.encounters = encounters;
    app.current_encounter_id = current_encounter_id;
    app.encounter_counter = encounter_counter;
    
    eframe::run_native(
        "NWN Log Overlay",
        native_options,
        Box::new(|cc| {
            // Set up dark theme consistently across platforms
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(app))
        }),
    )?;

    Ok(())
}