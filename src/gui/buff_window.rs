use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::models::{BuffTracker, AppSettings};

/// Show the buff window as a separate viewport (independent window)
pub fn show_buff_window(
    ctx: &egui::Context,
    buff_tracker: Arc<Mutex<BuffTracker>>,
    settings: Arc<Mutex<AppSettings>>,
    is_open: &mut bool
) {
    if !*is_open {
        return;
    }

    // Calculate window size based on number of buffs
    let (window_width, window_height) = if let Ok(tracker) = buff_tracker.lock() {
        let active_buffs = tracker.get_active_buffs();
        let buff_count = active_buffs.len();

        if buff_count == 0 {
            (100.0, 40.0)  // Minimal size for "No buffs"
        } else {
            // Calculate width based on longest buff name + time text (shorter format)
            let max_width = active_buffs.iter()
                .map(|buff| {
                    let remaining = buff.remaining_seconds();
                    let minutes = remaining / 60;
                    let seconds = remaining % 60;
                    let time_text = if minutes > 0 {
                        format!("{}: {}m {}s", buff.name, minutes, seconds)
                    } else {
                        format!("{}: {}s", buff.name, seconds)
                    };
                    time_text.len() as f32 * 8.0  // More generous character width for font size 16
                })
                .fold(0.0f32, |a, b| a.max(b));

            let width = (max_width + 30.0).clamp(150.0, 400.0);  // More padding and larger range
            let height = buff_count as f32 * 20.0 + 20.0;        // More generous line height for font size 16
            (width, height)
        }
    } else {
        (200.0, 60.0)  // Default if can't access tracker
    };

    // Get saved position from settings
    let initial_pos = if let Ok(settings_guard) = settings.lock() {
        settings_guard.buff_window_pos
    } else {
        None
    };

    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([window_width, window_height])  // No space needed for title bar
        .with_always_on_top()
        .with_resizable(false)  // Make non-resizable for clean auto-sizing
        .with_decorations(false);  // Remove system decorations

    // Set initial position if available
    if let Some((x, y)) = initial_pos {
        viewport_builder = viewport_builder.with_position([x, y]);
    }

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("buffs"),
        viewport_builder,
        |ctx, class| {
            assert!(class == egui::ViewportClass::Immediate);
            ctx.set_visuals(egui::Visuals::dark());

            egui::CentralPanel::default().show(ctx, |ui| {
                // Remove default margins and spacing for compact layout
                ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 2.0);
                ui.spacing_mut().indent = 0.0;

                // Make the window draggable by detecting drag on the UI background
                let ui_response = ui.interact(ui.max_rect(), egui::Id::new("buff_window_drag"), egui::Sense::click_and_drag());
                if ui_response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // Remove expired buffs first
                if let Ok(mut tracker) = buff_tracker.lock() {
                    tracker.remove_expired_buffs();

                    let active_buffs = tracker.get_active_buffs();

                    if !active_buffs.is_empty() {
                        for buff in active_buffs {
                            let remaining = buff.remaining_seconds();
                            if remaining > 0 {
                                let minutes = remaining / 60;
                                let seconds = remaining % 60;
                                let time_text = if minutes > 0 {
                                    format!("{}: {}m {}s", buff.name, minutes, seconds)  // Shorter format
                                } else {
                                    format!("{}: {}s", buff.name, seconds)              // Shorter format
                                };

                                // Check if we should flash this buff (expiring soon)
                                let should_flash = if let Ok(settings) = settings.lock() {
                                    remaining <= settings.buff_warning_seconds as i64
                                } else {
                                    false
                                };

                                if should_flash {
                                    // Create a flashing effect using animation time
                                    let time = ui.ctx().input(|i| i.time);
                                    let flash_intensity = ((time * 3.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                                    let flash_color = egui::Color32::from_rgb(
                                        ((155.0 * flash_intensity) as u8 + 100).min(255),
                                        ((100.0 * (1.0 - flash_intensity)) as u8 + 50).min(255),
                                        50
                                    );
                                    ui.add(egui::Label::new(egui::RichText::new(time_text).size(16.0).color(flash_color)).selectable(false));
                                    // Request repaint for animation
                                    ui.ctx().request_repaint();
                                } else {
                                    ui.add(egui::Label::new(egui::RichText::new(time_text).size(16.0)).selectable(false));
                                }
                            }
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new("No buffs").size(16.0)).selectable(false));
                        });
                    }
                } else {
                    ui.add(egui::Label::new(egui::RichText::new("Unable to access buff tracker").size(16.0)).selectable(false));
                }

                // Save window position if it changed
                if let Some(outer_rect) = ctx.input(|i| i.viewport().outer_rect) {
                    let current_pos = (outer_rect.min.x, outer_rect.min.y);
                    if let Ok(mut settings_guard) = settings.lock() {
                        if settings_guard.buff_window_pos != Some(current_pos) {
                            settings_guard.buff_window_pos = Some(current_pos);
                            // Auto-save settings
                            drop(settings_guard);
                            let settings_to_save = if let Ok(guard) = settings.lock() {
                                guard.clone()
                            } else {
                                return;
                            };
                            crate::utils::settings_persistence::auto_save_app_settings(&settings_to_save);
                        }
                    }
                }
            });
        },
    );
}