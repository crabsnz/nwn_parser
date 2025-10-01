use std::collections::HashMap;
use eframe::egui;
use crate::models::{CombatantStats, ViewMode};
use crate::gui::app::NwnLogApp;
use crate::utils::auto_save_app_settings;
use crate::log::finder::get_default_log_directory;

impl NwnLogApp {
    pub fn display_stats(&mut self, ui: &mut egui::Ui, stats_map: &HashMap<String, CombatantStats>) {
        // Update cache only if data changed
        self.update_sorted_cache(stats_map);

        // Calculate total and max damage from the DISPLAYED/FILTERED combatants only
        let total_encounter_damage: u32 = match self.damage_view_mode {
            crate::models::DamageViewMode::DamageDone => {
                self.cached_sorted_combatants.iter().map(|(_, s)| s.total_damage_dealt).sum()
            },
            crate::models::DamageViewMode::DamageTaken => {
                self.cached_sorted_combatants.iter().map(|(_, s)| s.total_damage_received).sum()
            }
        };

        // Find the maximum damage for bar scaling from displayed combatants only
        let max_damage = match self.damage_view_mode {
            crate::models::DamageViewMode::DamageDone => {
                self.cached_sorted_combatants.iter().map(|(_, s)| s.total_damage_dealt).max().unwrap_or(1)
            },
            crate::models::DamageViewMode::DamageTaken => {
                self.cached_sorted_combatants.iter().map(|(_, s)| s.total_damage_received).max().unwrap_or(1)
            }
        };
        
        // Use scrollable area and collapsible headers that scales with window size
        let available_height = ui.available_height().max(200.0);
        egui::ScrollArea::both()
            .max_height(available_height - 20.0) // Leave some padding
            .auto_shrink([false; 2])
            .show(ui, |ui| {
            for (name, stats) in &self.cached_sorted_combatants {
                // Calculate damage percentage for this player based on current view mode
                let (current_damage, percentage_denominator) = match self.damage_view_mode {
                    crate::models::DamageViewMode::DamageDone => {
                        (stats.total_damage_dealt, total_encounter_damage)
                    },
                    crate::models::DamageViewMode::DamageTaken => {
                        (stats.total_damage_received, total_encounter_damage)
                    }
                };

                let damage_percentage = if percentage_denominator > 0 && current_damage > 0 {
                    (current_damage as f32 / percentage_denominator as f32 * 100.0) as u32
                } else {
                    0
                };
                
                // Simple clickable row (no collapsing)
                let _id = egui::Id::new(name);

                // Get damage value based on current view mode
                let display_damage = match self.damage_view_mode {
                    crate::models::DamageViewMode::DamageDone => {
                        stats.total_damage_dealt
                    },
                    crate::models::DamageViewMode::DamageTaken => {
                        stats.total_damage_received
                    }
                };

                // Calculate bar width based on damage relative to max damage
                let bar_percentage = if max_damage > 0 {
                    display_damage as f32 / max_damage as f32
                } else {
                    0.0
                };
                
                // Reserve space for the header
                let header_height = 24.0;
                let available_width = ui.available_width();
                let header_rect = ui.allocate_space(egui::Vec2::new(available_width, header_height)).1;
                
                // Draw the progress bar background (green for players, red for NPCs/monsters)
                if bar_percentage > 0.0 {
                    let bar_width = available_width * bar_percentage;
                    let bar_rect = egui::Rect::from_min_size(
                        header_rect.min,
                        egui::Vec2::new(bar_width, header_height)
                    );

                    // Check if this is a known player
                    let is_player = if let Ok(registry) = self.player_registry.lock() {
                        registry.is_player(name)
                    } else {
                        false
                    };

                    let bar_color = if is_player {
                        egui::Color32::from_rgb(50, 150, 50)  // Green for players
                    } else {
                        egui::Color32::from_rgb(150, 50, 50)  // Red for NPCs/monsters
                    };

                    ui.painter().rect_filled(bar_rect, 2.0, bar_color);
                }
                
                // Create a clickable header that covers the entire area
                let header_response = ui.allocate_rect(header_rect, egui::Sense::click());
                
                // Draw the header content on top of the bar (non-interactive)
                let text_painter = ui.painter();

                // Draw player name in white
                let name_pos = egui::Pos2::new(header_rect.min.x + 15.0, header_rect.center().y);
                text_painter.text(name_pos, egui::Align2::LEFT_CENTER, name.clone(),
                    egui::FontId::proportional(14.0), egui::Color32::WHITE);

                // Draw damage info on the right in white
                if display_damage > 0 {
                    let damage_info = if let Some(dps) = stats.calculate_dps() {
                        format!("{} ({:.1}, {}%)", display_damage, dps, damage_percentage)
                    } else {
                        format!("{} ({}%)", display_damage, damage_percentage)
                    };
                    let damage_pos = egui::Pos2::new(header_rect.max.x - 8.0, header_rect.center().y);
                    text_painter.text(damage_pos, egui::Align2::RIGHT_CENTER, damage_info,
                        egui::FontId::proportional(14.0), egui::Color32::WHITE);
                }
                
                // Handle click to open player details window
                if header_response.clicked() {
                    let is_open = self.open_detail_windows.entry(name.clone()).or_insert(false);
                    *is_open = true;
                }
            }
        });
    }
}

impl eframe::App for NwnLogApp {
    /// This function is called on every frame to update the GUI.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request continuous repaints to keep the app updating even when not focused
        ctx.request_repaint();
        egui::CentralPanel::default().show(ctx, |ui| {
            // Prevent the UI from auto-sizing by setting a minimum size
            ui.set_min_size(egui::Vec2::new(400.0, 200.0));
            
            // Custom header bar
            let header_rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), 35.0)).1;
            
            // Make the entire header draggable except for the buttons
            let draggable_rect = egui::Rect::from_min_size(
                header_rect.min,
                egui::Vec2::new(header_rect.width() - 175.0, header_rect.height()) // More space for options button
            );
            let drag_response = ui.allocate_rect(draggable_rect, egui::Sense::click_and_drag());
            if drag_response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            
            // Draw the header content
            ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                ui.horizontal(|ui| {
                    // Left-aligned title (draggable area) - draw as non-interactive text
                    let title_pos = egui::Pos2::new(header_rect.min.x + 15.0, header_rect.center().y);

                    // Get the title - show player info if available, otherwise prompt to chat
                    let title = if let Ok(registry) = self.player_registry.lock() {
                        if let Some((account, character)) = registry.get_main_player_info() {
                            format!("[{}] {}", account, character)
                        } else {
                            // Show just account if no character yet
                            if let Some(account) = &registry.main_player_account {
                                format!("[{}]", account)
                            } else {
                                "Type in chat to activate buffs".to_string()
                            }
                        }
                    } else {
                        "Type in chat to activate buffs".to_string()
                    };

                    ui.painter().text(title_pos, egui::Align2::LEFT_CENTER, title,
                        egui::FontId::proportional(16.0), ui.visuals().text_color());
                    
                    // Push buttons to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        if ui.add(egui::Button::new(egui::RichText::new("X").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        
                        // Minimize button
                        if ui.add(egui::Button::new(egui::RichText::new("−").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }

                        // Options button (cog wheel) - next to minimize button
                        if ui.add(egui::Button::new(egui::RichText::new("⚙").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            self.show_options = !self.show_options;
                        }

                        // Add gap between minimize/close and font buttons
                        ui.add_space(10.0);
                        
                        // Font size decrease button (smaller A)
                        if ui.add(egui::Button::new(egui::RichText::new("A").size(8.0))
                            .min_size(egui::Vec2::new(18.0, 18.0))).clicked() {
                            self.text_scale = (self.text_scale - 0.1).max(0.5);
                            ctx.set_zoom_factor(ctx.zoom_factor() * 0.9);
                        }
                        
                        // Font size increase button (large A) - placed closer to small A
                        if ui.add(egui::Button::new(egui::RichText::new("A").size(16.0))
                            .min_size(egui::Vec2::new(20.0, 20.0))).clicked() {
                            self.text_scale = (self.text_scale + 0.1).min(2.0);
                            ctx.set_zoom_factor(ctx.zoom_factor() * 1.1);
                        }

                        // Add gap between font buttons and minimize button
                        ui.add_space(10.0);

                        // Minimize/expand rows button
                        let minimize_text = if self.rows_minimized { "▼" } else { "▲" };
                        if ui.add(egui::Button::new(egui::RichText::new(minimize_text).size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            self.rows_minimized = !self.rows_minimized;
                        }

                        // Add gap between minimize button and minimize/close buttons
                        ui.add_space(10.0);
                    });
                });
            });
            
            ui.separator();

            // Show button rows only if not minimized
            if !self.rows_minimized {
                // View mode selector
                ui.horizontal(|ui| {
                // Fixed width buttons for view modes
                // Determine what data is currently being shown to highlight the correct button
                let showing_encounters = !self.selected_encounter_ids.is_empty();
                let showing_current_fight = self.selected_encounter_ids.is_empty() && self.view_mode == ViewMode::CurrentFight;
                let showing_overall_stats = self.selected_encounter_ids.is_empty() && self.view_mode == ViewMode::OverallStats;

                if ui.add_sized([60.0, 20.0], egui::Button::new("Current").selected(showing_current_fight)).clicked() {
                    self.view_mode = ViewMode::CurrentFight;
                    self.selected_encounter_ids.clear(); // Clear encounter selections when switching to Current Fight
                }
                if ui.add_sized([60.0, 20.0], egui::Button::new("Overall").selected(showing_overall_stats)).clicked() {
                    self.view_mode = ViewMode::OverallStats;
                    self.selected_encounter_ids.clear(); // Clear encounter selections when switching to Overall Stats
                }
                let encounters_button_text = if !self.selected_encounter_ids.is_empty() {
                    format!("Encounters ({})", self.selected_encounter_ids.len())
                } else {
                    "Encounters".to_string()
                };
                if ui.add_sized([100.0, 20.0], egui::Button::new(encounters_button_text).selected(showing_encounters || self.view_mode == ViewMode::MultipleSelected)).clicked() {
                    if self.view_mode == ViewMode::MultipleSelected {
                        // If already in multi-select mode, close the selection UI but keep showing the data
                        self.view_mode = ViewMode::CurrentFight; // This will be overridden by get_current_stats if encounters are selected
                    } else {
                        // Switch to multi-select mode to show the selection UI
                        self.view_mode = ViewMode::MultipleSelected;
                    }
                }

                // Buffs button
                if ui.add_sized([55.0, 20.0], egui::Button::new("Buffs").selected(self.buff_window_spawned)).clicked() {
                    self.buff_window_spawned = !self.buff_window_spawned;
                }

                // Logs button
                if ui.add_sized([55.0, 20.0], egui::Button::new("Logs").selected(self.logs_window_open)).clicked() {
                    self.logs_window_open = !self.logs_window_open;
                }
            });

            // Second row: Damage view mode and filter buttons
            ui.horizontal(|ui| {
                ui.label("View:");

                // Damage view mode buttons
                if ui.add_sized([90.0, 20.0], egui::Button::new("Damage Done")
                    .selected(self.damage_view_mode == crate::models::DamageViewMode::DamageDone)).clicked() {
                    self.damage_view_mode = crate::models::DamageViewMode::DamageDone;
                }
                if ui.add_sized([95.0, 20.0], egui::Button::new("Damage Taken")
                    .selected(self.damage_view_mode == crate::models::DamageViewMode::DamageTaken)).clicked() {
                    self.damage_view_mode = crate::models::DamageViewMode::DamageTaken;
                }

                ui.add_space(20.0);
                ui.label("Filter:");

                // Combatant filter dropdown
                let filter_text = match self.combatant_filter {
                    crate::models::CombatantFilter::All => "All",
                    crate::models::CombatantFilter::Friendlies => "Friendlies",
                    crate::models::CombatantFilter::Enemies => "Enemies",
                };

                egui::ComboBox::from_label("")
                    .selected_text(filter_text)
                    .width(85.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.combatant_filter, crate::models::CombatantFilter::All, "All");
                        ui.selectable_value(&mut self.combatant_filter, crate::models::CombatantFilter::Friendlies, "Friendlies");
                        ui.selectable_value(&mut self.combatant_filter, crate::models::CombatantFilter::Enemies, "Enemies");
                    });
            });
            }

            // Show encounter selection UI if in multi-select mode
            if self.view_mode == ViewMode::MultipleSelected {
                ui.separator();
                ui.label("Select encounters to combine:");
                
                egui::ScrollArea::vertical()
                    .id_salt("encounter_selection_scroll")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        if let Ok(encounters) = self.encounters.try_lock() {
                            let mut sorted_encounters: Vec<_> = encounters.values().collect();
                            sorted_encounters.sort_by(|a, b| b.end_time.cmp(&a.end_time));
                            
                            for encounter in sorted_encounters {
                                let mut is_selected = self.selected_encounter_ids.contains(&encounter.id);
                                let display_name = encounter.get_display_name();
                                
                                // Make the entire row clickable by using a horizontal layout
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut is_selected, "").changed() {
                                        if is_selected {
                                            self.selected_encounter_ids.insert(encounter.id);
                                        } else {
                                            self.selected_encounter_ids.remove(&encounter.id);
                                        }
                                    }
                                    
                                    // Make the text also clickable
                                    let text_response = ui.selectable_label(is_selected, display_name);
                                    if text_response.clicked() {
                                        if is_selected {
                                            self.selected_encounter_ids.remove(&encounter.id);
                                        } else {
                                            self.selected_encounter_ids.insert(encounter.id);
                                        }
                                    }
                                });
                            }
                        }
                    });
                
                // Show selection summary
                if !self.selected_encounter_ids.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Selected {} encounter(s)", self.selected_encounter_ids.len()));
                        
                        if ui.button("Clear All").clicked() {
                            self.selected_encounter_ids.clear();
                        }
                        if ui.button("Select All").clicked() {
                            if let Ok(encounters) = self.encounters.try_lock() {
                                for encounter_id in encounters.keys() {
                                    self.selected_encounter_ids.insert(*encounter_id);
                                }
                            }
                        }
                    });
                }
            }
            
            ui.separator();
            
            // Get the stats data using the new system
            let stats_to_display = Some(self.get_current_stats());
            
            // Now display the UI with the collected data
            if let Some(stats_map) = stats_to_display {
                self.display_stats(ui, &stats_map);
            }
        }); // End CentralPanel

        // Render resize grip OUTSIDE the panel to ensure it's truly on top
        let window_rect = ctx.screen_rect();
        let grip_size = 15.0; // Slightly larger for easier grabbing

        // Bottom-right corner resize grip only
        let grip_rect = egui::Rect::from_min_size(
            egui::Pos2::new(window_rect.max.x - grip_size, window_rect.max.y - grip_size),
            egui::Vec2::new(grip_size, grip_size)
        );

        // Create a top-layer area for the resize grip to ensure it's above everything
        egui::Area::new(egui::Id::new("resize_grip_area"))
            .fixed_pos(grip_rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                let grip_id = egui::Id::new("resize_grip");
                let grip_response = ui.interact(grip_rect, grip_id, egui::Sense::click_and_drag());

                if grip_response.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeSouthEast);
                }

                if grip_response.dragged() {
                    let delta = grip_response.drag_delta();
                    if delta.length() > 0.1 { // More sensitive to small movements
                        // Get current window size more reliably
                        let current_size = ctx.input(|i| {
                            if let Some(rect) = i.viewport().inner_rect {
                                rect.size()
                            } else {
                                // Fallback to screen rect size
                                i.screen_rect().size()
                            }
                        });

                        let new_width = (current_size.x + delta.x).clamp(300.0, 1600.0);
                        let new_height = (current_size.y + delta.y).clamp(200.0, 1200.0);

                        // Send the resize command
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(new_width, new_height)));

                        // Force a repaint to make the resize more responsive
                        ctx.request_repaint();
                    }
                }

                // Draw resize grip indicator
                let grip_color = if grip_response.hovered() {
                    egui::Color32::from_gray(150)
                } else {
                    egui::Color32::from_gray(100)
                };

                // Draw resize grip lines
                let painter = ui.painter();
                for i in 0..3 {
                    let offset = i as f32 * 3.0;
                    let start = egui::Pos2::new(grip_rect.max.x - 2.0 - offset, grip_rect.min.y + offset + 2.0);
                    let end = egui::Pos2::new(grip_rect.min.x + offset + 2.0, grip_rect.max.y - 2.0 - offset);
                    painter.line_segment([start, end], egui::Stroke::new(1.5, grip_color));
                }
            });

        // Show options window if requested
        if self.show_options {
            self.show_options_window(ctx);
        }

        // Show independent buff window if requested
        if self.buff_window_spawned {
            if let Some(settings_ref) = &self.settings_ref {
                crate::gui::show_buff_window(ctx, self.buff_tracker.clone(),
                    settings_ref.clone(),
                    &mut self.buff_window_spawned);
            }
        }

        // Show logs window if requested
        if self.logs_window_open {
            if let Some(settings_ref) = &self.settings_ref {
                crate::gui::show_logs_window(ctx, &mut self.logs_window_state,
                    settings_ref.clone(),
                    &mut self.logs_window_open);
            }
        }

        // Show player detail windows
        let current_stats = self.get_current_stats();
        let mut windows_to_close = Vec::new();

        for (player_name, is_open) in self.open_detail_windows.iter_mut() {
            if *is_open {
                if let Some(stats) = current_stats.get(player_name) {
                    crate::gui::show_player_details_window(
                        ctx,
                        player_name,
                        stats,
                        self.player_registry.clone(),
                        &current_stats,
                        is_open
                    );
                } else {
                    // Player no longer exists in current stats, close the window
                    windows_to_close.push(player_name.clone());
                }
            }
        }

        // Clean up closed windows
        for player_name in windows_to_close {
            self.open_detail_windows.remove(&player_name);
        }
    }
}

impl NwnLogApp {
    /// Show the options configuration window
    fn show_options_window(&mut self, ctx: &egui::Context) {
        let mut show_options = self.show_options;
        egui::Window::new("Options")
            .open(&mut show_options)
            .default_size([300.0, 200.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.heading("Character Settings");
                ui.separator();

                // Caster Level setting
                ui.horizontal(|ui| {
                    ui.label("Caster Level:");
                    if let Some(settings_ref) = &self.settings_ref {
                        if let Ok(mut settings) = settings_ref.lock() {
                            let mut caster_level = settings.caster_level;
                            if ui.add(egui::DragValue::new(&mut caster_level).range(1..=40).speed(1.0)).changed() {
                                settings.set_caster_level(caster_level);
                                auto_save_app_settings(&*settings);
                            }
                        }
                    }
                });

                // Charisma Modifier setting
                ui.horizontal(|ui| {
                    ui.label("CHA Modifier:");
                    if let Some(settings_ref) = &self.settings_ref {
                        if let Ok(mut settings) = settings_ref.lock() {
                            let mut cha_mod = settings.charisma_modifier;
                            if ui.add(egui::DragValue::new(&mut cha_mod).range(-10..=50).speed(1.0)).changed() {
                                settings.set_charisma_modifier(cha_mod);
                                auto_save_app_settings(&*settings);
                            }
                        }
                    }
                });

                ui.add_space(10.0);
                ui.heading("Feats");
                ui.separator();

                // Extended Divine Might toggle
                if let Some(settings_ref) = &self.settings_ref {
                    if let Ok(mut settings) = settings_ref.lock() {
                        let mut extended_divine_might = settings.extended_divine_might;
                        if ui.checkbox(&mut extended_divine_might, "Extended Divine Might").changed() {
                            settings.extended_divine_might = extended_divine_might;
                            auto_save_app_settings(&*settings);
                        }
                    }
                }

                // Extended Divine Shield toggle
                if let Some(settings_ref) = &self.settings_ref {
                    if let Ok(mut settings) = settings_ref.lock() {
                        let mut extended_divine_shield = settings.extended_divine_shield;
                        if ui.checkbox(&mut extended_divine_shield, "Extended Divine Shield").changed() {
                            settings.extended_divine_shield = extended_divine_shield;
                            auto_save_app_settings(&*settings);
                        }
                    }
                }

                ui.add_space(10.0);
                ui.separator();

                // Buff Warning Time setting
                ui.label("Buff Warning Time (seconds):");
                if let Some(settings_ref) = &self.settings_ref {
                    if let Ok(mut settings) = settings_ref.lock() {
                        let mut warning_seconds = settings.buff_warning_seconds as i32;
                        if ui.add(egui::Slider::new(&mut warning_seconds, 1..=30).text("seconds")).changed() {
                            settings.set_buff_warning_seconds(warning_seconds as u32);
                            auto_save_app_settings(&*settings);
                        }
                    }
                }

                ui.add_space(10.0);
                ui.heading("Log Directory");
                ui.separator();

                // Log Directory setting
                ui.horizontal(|ui| {
                    ui.label("Log Directory:");
                    if let Some(settings_ref) = &self.settings_ref {
                        if let Ok(settings) = settings_ref.lock() {
                            // Use pending directory if available, otherwise current setting
                            let mut log_dir_text = self.pending_log_directory.clone()
                                .or_else(|| settings.log_directory.clone())
                                .unwrap_or_default();

                            let text_edit = ui.text_edit_singleline(&mut log_dir_text);

                            if text_edit.changed() {
                                // Store as pending, don't save yet
                                self.pending_log_directory = if log_dir_text.trim().is_empty() {
                                    None
                                } else {
                                    Some(log_dir_text)
                                };
                                self.show_log_dir_confirm = true;
                            }

                            // Show confirmation buttons if there's a pending change
                            if self.show_log_dir_confirm {
                                ui.separator();

                                // Confirm button (green checkmark)
                                if ui.button("✓ Confirm").clicked() {
                                    // Clear all existing data immediately
                                    if let Ok(mut encounters) = self.encounters.lock() {
                                        encounters.clear();
                                    }
                                    if let Ok(mut current_id) = self.current_encounter_id.lock() {
                                        *current_id = None;
                                    }
                                    if let Ok(mut counter) = self.encounter_counter.lock() {
                                        *counter = 1;
                                    }

                                    // Apply the pending change
                                    drop(settings); // Release the lock
                                    if let Ok(mut settings) = settings_ref.lock() {
                                        settings.log_directory = self.pending_log_directory.clone();
                                        auto_save_app_settings(&*settings);
                                    }

                                    // Signal log reload
                                    if let Ok(mut reload_flag) = self.log_reload_requested.lock() {
                                        *reload_flag = true;
                                    }

                                    // Clear pending state
                                    self.pending_log_directory = None;
                                    self.show_log_dir_confirm = false;
                                }

                                // Cancel button (red X)
                                if ui.button("✗ Cancel").clicked() {
                                    // Revert pending change
                                    self.pending_log_directory = None;
                                    self.show_log_dir_confirm = false;
                                }
                            } else {
                                // Reset to auto-detection button (only when not in confirm mode and not already default)
                                let current_dir = settings.log_directory.as_ref();
                                let default_dir = get_default_log_directory();

                                // Only show button if current setting differs from auto-detected default
                                if current_dir != default_dir.as_ref() {
                                    if ui.button("Reset to Auto").clicked() {
                                        if let Some(default_dir) = default_dir {
                                            self.pending_log_directory = Some(default_dir);
                                            self.show_log_dir_confirm = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                });

                // Show current auto-detected path as help text
                if let Some(default_dir) = get_default_log_directory() {
                    ui.small(format!("Auto-detected: {}", default_dir));
                } else {
                    ui.small("No log directory auto-detected");
                }

                // Display current settings info
                ui.add_space(10.0);
                ui.separator();
                ui.label(format!(
                    "Settings saved to: settings.json"
                ));
            });

        self.show_options = show_options;
    }

    // Buff window is now handled as an independent application - no embedded window needed

}