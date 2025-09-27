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
                    time_text.len() as f32 * 6.8  // Slightly tighter character width
                })
                .fold(0.0f32, |a, b| a.max(b));

            let width = (max_width + 15.0).clamp(120.0, 300.0);  // Smaller min/max
            let height = buff_count as f32 * 16.0 + 15.0;        // Tighter line height
            (width, height)
        }
    } else {
        (200.0, 60.0)  // Default if can't access tracker
    };

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("buff_window"),
        egui::ViewportBuilder::default()
            .with_inner_size([window_width, window_height + 35.0])  // Add space for custom title bar
            .with_always_on_top()
            .with_resizable(false)  // Make non-resizable for clean auto-sizing
            .with_decorations(false),  // Remove system decorations for custom title bar
        |ctx, class| {
            assert!(class == egui::ViewportClass::Immediate);
            ctx.set_visuals(egui::Visuals::dark());

            egui::CentralPanel::default().show(ctx, |ui| {
                // Custom header bar (like main window)
                let header_rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), 35.0)).1;

                // Make the header draggable except for the X button area
                let draggable_rect = egui::Rect::from_min_size(
                    header_rect.min,
                    egui::Vec2::new(header_rect.width() - 30.0, header_rect.height()) // Leave space for X button
                );
                let drag_response = ui.allocate_rect(draggable_rect, egui::Sense::click_and_drag());
                if drag_response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // Draw the header content
                ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                    ui.horizontal(|ui| {
                        // Draw "Buffs" title on the left
                        let title_pos = egui::Pos2::new(header_rect.min.x + 15.0, header_rect.center().y);
                        ui.painter().text(title_pos, egui::Align2::LEFT_CENTER, "Buffs",
                            egui::FontId::proportional(16.0), ui.visuals().text_color());

                        // Push X button to the right
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Close button (X)
                            if ui.add(egui::Button::new(egui::RichText::new("X").size(12.0))
                                .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                                *is_open = false;
                            }
                        });
                    });
                });

                // Separator line between header and content
                ui.add_space(2.0);
                ui.separator();
                ui.add_space(2.0);

                // Remove default margins and spacing for compact layout
                ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 2.0);
                ui.spacing_mut().indent = 0.0;

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
                                    ui.colored_label(flash_color, time_text);
                                    // Request repaint for animation
                                    ui.ctx().request_repaint();
                                } else {
                                    ui.label(time_text);
                                }
                            }
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("No buffs");  // Shorter text
                        });
                    }
                } else {
                    ui.label("Unable to access buff tracker");
                }
            });
        },
    );
}