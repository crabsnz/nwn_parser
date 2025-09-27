use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::models::{CombatantStats, PlayerRegistry};

/// Show the player details window as a separate viewport (independent window)
pub fn show_player_details_window(
    ctx: &egui::Context,
    player_name: &str,
    stats: &CombatantStats,
    player_registry: Arc<Mutex<PlayerRegistry>>,
    all_stats: &std::collections::HashMap<String, CombatantStats>,
    is_open: &mut bool
) {
    if !*is_open {
        return;
    }

    // Calculate window size (compact for exactly 10 bars per column)
    let window_width = 800.0;
    let window_height = 400.0; // Reduced height for compact display

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of(format!("player_details_{}", player_name)),
        egui::ViewportBuilder::default()
            .with_inner_size([window_width, window_height + 35.0])  // Add space for custom title bar
            .with_always_on_top()
            .with_resizable(true)
            .with_decorations(false),  // Remove system decorations for custom title bar
        |ctx, class| {
            assert!(class == egui::ViewportClass::Immediate);
            ctx.set_visuals(egui::Visuals::dark());

            egui::CentralPanel::default().show(ctx, |ui| {
                // Custom header bar
                let header_rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), 35.0)).1;

                // Make the header draggable except for the X button area
                let draggable_rect = egui::Rect::from_min_size(
                    header_rect.min,
                    egui::Vec2::new(header_rect.width() - 30.0, header_rect.height())
                );
                let drag_response = ui.allocate_rect(draggable_rect, egui::Sense::click_and_drag());
                if drag_response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // Draw the header content
                ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                    ui.horizontal(|ui| {
                        // Draw player name and title on the left
                        let display_name = if let Ok(registry) = player_registry.lock() {
                            registry.get_display_name(player_name)
                        } else {
                            player_name.to_string()
                        };

                        let title_text = format!("Details: {}", display_name);
                        let title_pos = egui::Pos2::new(header_rect.min.x + 15.0, header_rect.center().y);
                        ui.painter().text(title_pos, egui::Align2::LEFT_CENTER, title_text,
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
                ui.add_space(5.0);

                // Split into two columns for damage done and damage taken
                ui.columns(2, |columns| {
                    // Left column: Damage Done Section
                    egui::Frame::default()
                        .inner_margin(egui::Margin::same(8))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
                        .show(&mut columns[0], |ui| {
                            ui.set_min_height(235.0); // Fixed height for 10 bars + heading + spacing
                            ui.heading("Damage Done");
                            ui.add_space(5.0);

                            egui::ScrollArea::vertical()
                                .id_salt("damage_done_scroll")
                                .max_height(200.0) // Height for exactly 10 items (10 × 20px)
                                .show(ui, |ui| {

                                // Damage by target with bar graphs and hover tooltips
                                if !stats.damage_by_target_dealt.is_empty() {
                                    let mut sorted_targets: Vec<_> = stats.damage_by_target_dealt.iter().collect();
                                    sorted_targets.sort_by(|a, b| b.1.cmp(a.1)); // Sort by damage descending

                                    for (target, amount) in sorted_targets {
                                        let percentage = if stats.total_damage_dealt > 0 {
                                            *amount as f32 / stats.total_damage_dealt as f32
                                        } else {
                                            0.0
                                        };

                                        // Reserve space for the target row
                                        let row_height = 20.0;
                                        let available_width = ui.available_width();
                                        let row_rect = ui.allocate_space(egui::Vec2::new(available_width, row_height)).1;

                                        // Draw the percentage bar background (green for damage done)
                                        if percentage > 0.0 {
                                            let bar_width = available_width * percentage;
                                            let bar_rect = egui::Rect::from_min_size(
                                                row_rect.min,
                                                egui::Vec2::new(bar_width, row_height)
                                            );
                                            ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(50, 150, 50));
                                        }

                                        // Make the entire row interactive for hover and click for more responsive interaction
                                        let row_response = ui.allocate_rect(row_rect, egui::Sense::hover().union(egui::Sense::click()));

                                        // Draw target name on left side of the bar (truncate if too long)
                                        let display_name = if target.len() > 40 {
                                            format!("{}...", &target[..37])
                                        } else {
                                            target.clone()
                                        };
                                        let name_pos = egui::Pos2::new(row_rect.min.x + 5.0, row_rect.center().y);
                                        ui.painter().text(name_pos, egui::Align2::LEFT_CENTER, display_name,
                                            egui::FontId::proportional(12.0), egui::Color32::WHITE);

                                        // Draw damage info on right side of the bar
                                        let dps = if let Some(base_dps) = stats.calculate_dps() {
                                            if stats.total_damage_dealt > 0 {
                                                base_dps * (*amount as f64 / stats.total_damage_dealt as f64)
                                            } else {
                                                0.0
                                            }
                                        } else {
                                            0.0
                                        };

                                        let damage_info = if dps > 0.0 {
                                            format!("{} ({:.1}, {:.1}%)", amount, dps, percentage * 100.0)
                                        } else {
                                            format!("{} ({:.1}%)", amount, percentage * 100.0)
                                        };

                                        let damage_pos = egui::Pos2::new(row_rect.max.x - 5.0, row_rect.center().y);
                                        ui.painter().text(damage_pos, egui::Align2::RIGHT_CENTER, damage_info,
                                            egui::FontId::proportional(11.0), egui::Color32::WHITE);

                                        // Show tooltip on hover with detailed breakdown
                                        row_response.on_hover_ui(|ui| {
                                            ui.label(egui::RichText::new(format!("Details for {}", target)).strong());
                                            ui.separator();

                                            // Show damage by source for this target
                                            if let Some(source_map) = stats.damage_by_target_and_source_dealt.get(target) {
                                                ui.label("Damage by Source:");
                                                let mut sorted_sources: Vec<_> = source_map.iter().collect();
                                                sorted_sources.sort_by(|a, b| b.1.cmp(a.1));
                                                for (source, src_amount) in sorted_sources {
                                                    ui.label(format!("  {}: {}", source, src_amount));
                                                }
                                                ui.add_space(5.0);
                                            }

                                            // Show damage by type for this target
                                            if let Some(source_type_map) = stats.damage_by_target_source_and_type_dealt.get(target) {
                                                ui.label("Damage by Type:");
                                                let mut type_totals: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
                                                for type_map in source_type_map.values() {
                                                    for (dtype, dtype_amount) in type_map {
                                                        *type_totals.entry(dtype.clone()).or_default() += *dtype_amount;
                                                    }
                                                }
                                                let mut sorted_types: Vec<_> = type_totals.iter().collect();
                                                sorted_types.sort_by(|a, b| b.1.cmp(a.1));
                                                for (dtype, type_amount) in sorted_types {
                                                    // For damage done to targets, show the target's absorbed damage
                                                    let absorbed_amount = if let Some(target_stats) = all_stats.get(target) {
                                                        target_stats.absorbed_by_type.get(dtype).copied().unwrap_or(0)
                                                    } else {
                                                        0
                                                    };

                                                    if absorbed_amount > 0 {
                                                        // Calculate percentage of this damage type that was absorbed
                                                        let total_type_damage = *type_amount + absorbed_amount;
                                                        let absorbed_percentage = if total_type_damage > 0 {
                                                            (absorbed_amount as f32 / total_type_damage as f32 * 100.0)
                                                        } else {
                                                            0.0
                                                        };
                                                        ui.label(format!("  {}: {} (absorbed: {} {:.1}%)", dtype, type_amount, absorbed_amount, absorbed_percentage));
                                                    } else {
                                                        ui.label(format!("  {}: {}", dtype, type_amount));
                                                    }
                                                }
                                            }
                                        });
                                    }
                                }
                            });
                        });

                    // Right column: Damage Taken Section
                    egui::Frame::default()
                        .inner_margin(egui::Margin::same(8))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
                        .show(&mut columns[1], |ui| {
                            ui.set_min_height(235.0); // Fixed height for 10 bars + heading + spacing
                            ui.heading("Damage Taken");
                            ui.add_space(5.0);

                            egui::ScrollArea::vertical()
                                .id_salt("damage_taken_scroll")
                                .max_height(200.0) // Height for exactly 10 items (10 × 20px)
                                .show(ui, |ui| {

                                // Damage by attacker with bar graphs and hover tooltips
                                if !stats.damage_by_attacker_received.is_empty() {
                                    let mut sorted_attackers: Vec<_> = stats.damage_by_attacker_received.iter().collect();
                                    sorted_attackers.sort_by(|a, b| b.1.cmp(a.1)); // Sort by damage descending

                                    for (attacker, amount) in sorted_attackers {
                                        let percentage = if stats.total_damage_received > 0 {
                                            *amount as f32 / stats.total_damage_received as f32
                                        } else {
                                            0.0
                                        };

                                        // Reserve space for the attacker row
                                        let row_height = 20.0;
                                        let available_width = ui.available_width();
                                        let row_rect = ui.allocate_space(egui::Vec2::new(available_width, row_height)).1;

                                        // Draw the percentage bar background (red for damage taken)
                                        if percentage > 0.0 {
                                            let bar_width = available_width * percentage;
                                            let bar_rect = egui::Rect::from_min_size(
                                                row_rect.min,
                                                egui::Vec2::new(bar_width, row_height)
                                            );
                                            ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(150, 50, 50));
                                        }

                                        // Make the entire row interactive for hover and click for more responsive interaction
                                        let row_response = ui.allocate_rect(row_rect, egui::Sense::hover().union(egui::Sense::click()));

                                        // Draw attacker name on left side of the bar (truncate if too long)
                                        let display_name = if attacker.len() > 40 {
                                            format!("{}...", &attacker[..37])
                                        } else {
                                            attacker.clone()
                                        };
                                        let name_pos = egui::Pos2::new(row_rect.min.x + 5.0, row_rect.center().y);
                                        ui.painter().text(name_pos, egui::Align2::LEFT_CENTER, display_name,
                                            egui::FontId::proportional(12.0), egui::Color32::WHITE);

                                        // Draw damage info on right side of the bar
                                        let dtps = if let Some(base_dtps) = stats.calculate_dtps() {
                                            if stats.total_damage_received > 0 {
                                                base_dtps * (*amount as f64 / stats.total_damage_received as f64)
                                            } else {
                                                0.0
                                            }
                                        } else {
                                            0.0
                                        };

                                        let damage_info = if dtps > 0.0 {
                                            format!("{} ({:.1}, {:.1}%)", amount, dtps, percentage * 100.0)
                                        } else {
                                            format!("{} ({:.1}%)", amount, percentage * 100.0)
                                        };

                                        let damage_pos = egui::Pos2::new(row_rect.max.x - 5.0, row_rect.center().y);
                                        ui.painter().text(damage_pos, egui::Align2::RIGHT_CENTER, damage_info,
                                            egui::FontId::proportional(11.0), egui::Color32::WHITE);

                                        // Show tooltip on hover with detailed breakdown
                                        row_response.on_hover_ui(|ui| {
                                            ui.label(egui::RichText::new(format!("Details for {}", attacker)).strong());
                                            ui.separator();

                                            // Show damage by source for this attacker
                                            if let Some(source_map) = stats.damage_by_attacker_and_source_received.get(attacker) {
                                                ui.label("Damage by Source:");
                                                let mut sorted_sources: Vec<_> = source_map.iter().collect();
                                                sorted_sources.sort_by(|a, b| b.1.cmp(a.1));
                                                for (source, src_amount) in sorted_sources {
                                                    ui.label(format!("  {}: {}", source, src_amount));
                                                }
                                                ui.add_space(5.0);
                                            }

                                            // Show damage types received from this attacker
                                            if let Some(source_map) = stats.damage_by_attacker_and_source_received.get(attacker) {
                                                ui.label("Damage by Type:");
                                                let mut type_totals: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

                                                // Aggregate damage types across all sources from this attacker
                                                for source_name in source_map.keys() {
                                                    // Construct the proper key format: "attacker (source)"
                                                    let received_source_key = format!("{} ({})", attacker, source_name);
                                                    if let Some(type_map) = stats.damage_by_source_and_type_received.get(&received_source_key) {
                                                        for (dtype, dtype_amount) in type_map {
                                                            *type_totals.entry(dtype.clone()).or_default() += *dtype_amount;
                                                        }
                                                    }
                                                }

                                                let mut sorted_types: Vec<_> = type_totals.iter().collect();
                                                sorted_types.sort_by(|a, b| b.1.cmp(a.1));
                                                for (dtype, type_amount) in sorted_types {
                                                    // Show absorbed damage inline with each damage type
                                                    let absorbed_amount = stats.absorbed_by_type.get(dtype).copied().unwrap_or(0);

                                                    if absorbed_amount > 0 {
                                                        // Calculate percentage of this damage type that was absorbed
                                                        let total_type_damage = *type_amount + absorbed_amount;
                                                        let absorbed_percentage = if total_type_damage > 0 {
                                                            absorbed_amount as f32 / total_type_damage as f32 * 100.0
                                                        } else {
                                                            0.0
                                                        };
                                                        ui.label(format!("  {}: {} (absorbed: {} {:.1}%)", dtype, type_amount, absorbed_amount, absorbed_percentage));
                                                    } else {
                                                        ui.label(format!("  {}: {}", dtype, type_amount));
                                                    }
                                                }
                                                ui.add_space(5.0);
                                            }
                                        });
                                    }
                                }
                            });
                        });
                });
            });
        },
    );
}