use std::collections::HashMap;
use eframe::egui;
use crate::models::{CombatantStats, ViewMode};
use crate::gui::app::NwnLogApp;

impl NwnLogApp {
    /// Helper function to create a custom collapsible header with full click area
    pub fn custom_collapsing_header(
        &self, 
        ui: &mut egui::Ui, 
        id: egui::Id, 
        text: &str,
        content: impl FnOnce(&mut egui::Ui)
    ) {
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        
        // Calculate header height and width
        let header_height = 20.0;
        let available_width = ui.available_width();
        let header_rect = ui.allocate_space(egui::Vec2::new(available_width, header_height)).1;
        
        // Create clickable header
        let header_response = ui.allocate_rect(header_rect, egui::Sense::click());
        
        // Draw header content (non-interactive)
        let text_painter = ui.painter();
        
        // Draw collapsing arrow
        let arrow_text = if state.is_open() { "▼" } else { "▶" };
        let arrow_pos = egui::Pos2::new(header_rect.min.x + 8.0, header_rect.center().y);
        text_painter.text(arrow_pos, egui::Align2::LEFT_CENTER, arrow_text, egui::FontId::default(), ui.visuals().text_color());
        
        // Draw header text
        let text_pos = egui::Pos2::new(header_rect.min.x + 25.0, header_rect.center().y);
        text_painter.text(text_pos, egui::Align2::LEFT_CENTER, text, 
            egui::FontId::proportional(13.0), ui.visuals().text_color());
        
        // Handle click
        if header_response.clicked() {
            state.toggle(ui);
        }
        
        state.store(ui.ctx());
        
        // Show content if expanded
        if state.is_open() {
            ui.indent(id, |ui| {
                content(ui);
            });
        }
    }

    pub fn display_stats(&mut self, ui: &mut egui::Ui, stats_map: &HashMap<String, CombatantStats>) {
        // Update cache only if data changed
        self.update_sorted_cache(stats_map);
        
        // Calculate total encounter damage for percentage calculation
        let total_encounter_damage: u32 = stats_map.values().map(|s| s.total_damage_dealt).sum();
        
        // Find the maximum damage for bar scaling
        let max_damage = stats_map.values().map(|s| s.total_damage_dealt).max().unwrap_or(1);
        
        // Use scrollable area and collapsible headers that scales with window size
        let available_height = ui.available_height().max(200.0);
        egui::ScrollArea::both()
            .max_height(available_height - 20.0) // Leave some padding
            .auto_shrink([false; 2])
            .show(ui, |ui| {
            for (name, stats) in &self.cached_sorted_combatants {
                // Calculate damage percentage for this player
                let damage_percentage = if total_encounter_damage > 0 && stats.total_damage_dealt > 0 {
                    (stats.total_damage_dealt as f32 / total_encounter_damage as f32 * 100.0) as u32
                } else {
                    0
                };
                
                // Custom collapsible header with right-aligned damage info and progress bar
                let id = egui::Id::new(name);
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
                
                // Calculate bar width based on damage relative to max damage
                let bar_percentage = if max_damage > 0 {
                    stats.total_damage_dealt as f32 / max_damage as f32
                } else {
                    0.0
                };
                
                // Reserve space for the header
                let header_height = 24.0;
                let available_width = ui.available_width();
                let header_rect = ui.allocate_space(egui::Vec2::new(available_width, header_height)).1;
                
                // Draw the red progress bar background
                if bar_percentage > 0.0 {
                    let bar_width = available_width * bar_percentage;
                    let bar_rect = egui::Rect::from_min_size(
                        header_rect.min,
                        egui::Vec2::new(bar_width, header_height)
                    );
                    ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(200, 50, 50));
                }
                
                // Create a clickable header that covers the entire area
                let header_response = ui.allocate_rect(header_rect, egui::Sense::click());
                
                // Draw the header content on top of the bar (non-interactive)
                let text_painter = ui.painter();
                
                // Draw collapsing arrow
                let arrow_text = if state.is_open() { "▼" } else { "▶" };
                let arrow_pos = egui::Pos2::new(header_rect.min.x + 8.0, header_rect.center().y - 6.0);
                text_painter.text(arrow_pos, egui::Align2::LEFT_CENTER, arrow_text, egui::FontId::default(), egui::Color32::WHITE);
                
                // Draw player name
                let name_pos = egui::Pos2::new(header_rect.min.x + 25.0, header_rect.center().y);
                text_painter.text(name_pos, egui::Align2::LEFT_CENTER, name.clone(), 
                    egui::FontId::proportional(14.0), egui::Color32::WHITE);
                
                // Draw damage info on the right
                if stats.total_damage_dealt > 0 {
                    let damage_info = if let Some(dps) = stats.calculate_dps() {
                        format!("{} ({:.1}, {}%)", stats.total_damage_dealt, dps, damage_percentage)
                    } else {
                        format!("{} ({}%)", stats.total_damage_dealt, damage_percentage)
                    };
                    let damage_pos = egui::Pos2::new(header_rect.max.x - 8.0, header_rect.center().y);
                    text_painter.text(damage_pos, egui::Align2::RIGHT_CENTER, damage_info, 
                        egui::FontId::proportional(14.0), egui::Color32::WHITE);
                }
                
                // Handle click to toggle
                if header_response.clicked() {
                    state.toggle(ui);
                }
                
                state.store(ui.ctx());
                
                // Show content if expanded
                if state.is_open() {
                    ui.indent(id, |ui| {
                        if stats.total_damage_dealt > 0 {
                            let damage_label = if let Some(dps) = stats.calculate_dps() {
                                format!("Total Damage Dealt: {} ({:.1} DPS)", stats.total_damage_dealt, dps)
                            } else {
                                format!("Total Damage Dealt: {}", stats.total_damage_dealt)
                            };
                            
                            self.custom_collapsing_header(ui, egui::Id::new(format!("{}_damage_dealt", name)), &damage_label, |ui| {
                                    // Sort targets by damage dealt to them (highest to lowest)
                                    let mut sorted_targets: Vec<_> = stats.damage_by_target_dealt.iter().collect();
                                    sorted_targets.sort_by(|a, b| b.1.cmp(a.1));
                                    
                                    for (target, target_damage) in sorted_targets {
                                        // Show each target as a collapsible header with total damage to that target
                                        let target_dps_text = if let Some(dps) = stats.calculate_source_dps(*target_damage) {
                                            format!("{}: {} ({:.1} DPS)", target, target_damage, dps)
                                        } else {
                                            format!("{}: {}", target, target_damage)
                                        };
                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_target_{}", name, target)), &target_dps_text, |ui| {
                                            // Show damage sources for this specific target, sorted by amount
                                            if let Some(source_map) = stats.damage_by_target_and_source_dealt.get(target) {
                                                let mut sorted_sources: Vec<_> = source_map.iter().collect();
                                                // Sort with Attack first, then spells alphabetically
                                                sorted_sources.sort_by(|a, b| {
                                                    if a.0 == "Attack" && b.0 != "Attack" {
                                                        std::cmp::Ordering::Less
                                                    } else if a.0 != "Attack" && b.0 == "Attack" {
                                                        std::cmp::Ordering::Greater
                                                    } else {
                                                        a.0.cmp(b.0)
                                                    }
                                                });
                                                
                                                for (source, amount) in sorted_sources {
                                                    let source_dps_text = if let Some(dps) = stats.calculate_source_dps(*amount) {
                                                        format!("{}: {} ({:.1} DPS)", source, amount, dps)
                                                    } else {
                                                        format!("{}: {}", source, amount)
                                                    };
                                                    self.custom_collapsing_header(ui, egui::Id::new(format!("{}_target_{}_source_{}", name, target, source)), &source_dps_text, |ui| {
                                                // Show detailed attack statistics if this is the "Attack" source
                                                if source == "Attack" {
                                                    // Calculate accurate average damage for hits and crits
                                                    let hit_avg = if stats.hits > 0 { stats.hit_damage / stats.hits } else { 0 };
                                                    let crit_avg = if stats.critical_hits > 0 { stats.crit_damage / stats.critical_hits } else { 0 };
                                                    
                                                    if stats.misses > 0 {
                                                        if stats.concealment_dodges > 0 {
                                                            ui.label(format!("Misses {} (concealment {})", stats.misses, stats.concealment_dodges));
                                                        } else {
                                                            ui.label(format!("{} Misses", stats.misses));
                                                        }
                                                    }
                                                    
                                                    if stats.hits > 0 {
                                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_hits", name)), &format!("{} Hits (avg {})", stats.hits, hit_avg), |ui| {
                                                                if let Some(type_map) = stats.hit_damage_by_target_type.get(target) {
                                                                    for (damage_type, type_amount) in type_map {
                                                                        ui.label(format!("{}: {}", damage_type, type_amount));
                                                                    }
                                                                }
                                                            });
                                                    }
                                                    
                                                    if stats.critical_hits > 0 {
                                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_crits", name)), &format!("{} Crits (avg {})", stats.critical_hits, crit_avg), |ui| {
                                                                if let Some(type_map) = stats.crit_damage_by_target_type.get(target) {
                                                                    for (damage_type, type_amount) in type_map {
                                                                        ui.label(format!("{}: {}", damage_type, type_amount));
                                                                    }
                                                                }
                                                            });
                                                    }
                                                    
                                                    if stats.weapon_buffs > 0 {
                                                        let buff_avg = if stats.weapon_buffs > 0 { stats.weapon_buff_damage / stats.weapon_buffs } else { 0 };
                                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_weapon_buffs", name)), &format!("{} Weapon Buff (avg {})", stats.weapon_buffs, buff_avg), |ui| {
                                                                if let Some(type_map) = stats.weapon_buff_damage_by_target_type.get(target) {
                                                                    for (damage_type, type_amount) in type_map {
                                                                        ui.label(format!("{}: {}", damage_type, type_amount));
                                                                    }
                                                                }
                                                            });
                                                    }
                                                } else {
                                                    // For non-Attack sources, show damage types for this specific target
                                                    if let Some(target_map) = stats.damage_by_target_source_and_type_dealt.get(target) {
                                                        if let Some(type_map) = target_map.get(source) {
                                                            for (damage_type, type_amount) in type_map {
                                                                ui.label(format!("{}: {}", damage_type, type_amount));
                                                            }
                                                        }
                                                    }
                                                    }
                                                });
                                                }
                                            }
                                        });
                                    }
                                });
                        }
                        if stats.total_damage_received > 0 || stats.total_damage_absorbed > 0 {
                            self.custom_collapsing_header(ui, egui::Id::new(format!("{}_damage_received", name)), &format!("Total Damage Received: {}", stats.total_damage_received), |ui| {
                                    // Sort attackers by damage received from them (highest to lowest)
                                    let mut sorted_attackers: Vec<_> = stats.damage_by_attacker_received.iter().collect();
                                    sorted_attackers.sort_by(|a, b| b.1.cmp(a.1));
                                    
                                    for (attacker, attacker_damage) in sorted_attackers {
                                        // Show each attacker as a collapsible header with total damage from that attacker
                                        self.custom_collapsing_header(ui, egui::Id::new(format!("{}_attacker_{}", name, attacker)), &format!("{}: {}", attacker, attacker_damage), |ui| {
                                            // Show damage sources from this specific attacker, sorted by amount
                                            if let Some(source_map) = stats.damage_by_attacker_and_source_received.get(attacker) {
                                                let mut sorted_sources: Vec<_> = source_map.iter().collect();
                                                sorted_sources.sort_by(|a, b| b.1.cmp(a.1));
                                                
                                                for (source, amount) in sorted_sources {
                                                    self.custom_collapsing_header(ui, egui::Id::new(format!("{}_attacker_{}_source_{}", name, attacker, source)), &format!("{}: {}", source, amount), |ui| {
                                                        // Show damage types for this attacker's source
                                                        let combined_source = format!("{} ({})", attacker, source);
                                                        if let Some(type_map) = stats.damage_by_source_and_type_received.get(&combined_source) {
                                                            for (damage_type, type_amount) in type_map {
                                                                let absorbed_amount = stats.absorbed_by_type.get(damage_type).unwrap_or(&0);
                                                                let total_attempted_type = type_amount + absorbed_amount;
                                                                
                                                                if *absorbed_amount > 0 {
                                                                    let immunity_percent = if total_attempted_type > 0 {
                                                                        (*absorbed_amount as f32 / total_attempted_type as f32 * 100.0) as u32
                                                                    } else {
                                                                        0
                                                                    };
                                                                    ui.label(format!("{}: {} ({} absorbed, {}% immunity)", 
                                                                        damage_type, type_amount, absorbed_amount, immunity_percent));
                                                                } else {
                                                                    ui.label(format!("{}: {}", damage_type, type_amount));
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                        });
                                    }
                                    
                                    // Show absorbed damage types that had no received damage (100% immunity)
                                    if !stats.absorbed_by_type.is_empty() {
                                        let mut all_received_types = std::collections::HashSet::new();
                                        for type_map in stats.damage_by_source_and_type_received.values() {
                                            for damage_type in type_map.keys() {
                                                all_received_types.insert(damage_type.clone());
                                            }
                                        }
                                        
                                        for (damage_type, absorbed_amount) in &stats.absorbed_by_type {
                                            if !all_received_types.contains(damage_type) {
                                                ui.label(format!("Complete Immunity - {}: 0 ({} absorbed, 100% immunity)", 
                                                    damage_type, absorbed_amount));
                                            }
                                        }
                                    }
                                });
                        }
                    });
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
            ui.set_min_size(egui::Vec2::new(300.0, 200.0));
            
            // Custom header bar
            let header_rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), 35.0)).1;
            
            // Make the entire header draggable except for the buttons
            let draggable_rect = egui::Rect::from_min_size(
                header_rect.min,
                egui::Vec2::new(header_rect.width() - 140.0, header_rect.height()) // More space for font buttons
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
                    ui.painter().text(title_pos, egui::Align2::LEFT_CENTER, "NWN Combat Tracker", 
                        egui::FontId::proportional(16.0), ui.visuals().text_color());
                    
                    // Push buttons to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        if ui.add(egui::Button::new(egui::RichText::new("✕").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        
                        // Minimize button  
                        if ui.add(egui::Button::new(egui::RichText::new("−").size(12.0))
                            .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
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
                    });
                });
            });
            
            ui.separator();
            
            // View mode selector
            ui.horizontal(|ui| {
                // Fixed width buttons for view modes
                // Determine what data is currently being shown to highlight the correct button
                let showing_encounters = !self.selected_encounter_ids.is_empty();
                let showing_current_fight = self.selected_encounter_ids.is_empty() && self.view_mode == ViewMode::CurrentFight;
                let showing_overall_stats = self.selected_encounter_ids.is_empty() && self.view_mode == ViewMode::OverallStats;
                
                if ui.add_sized([100.0, 20.0], egui::Button::new("Current Fight").selected(showing_current_fight)).clicked() {
                    self.view_mode = ViewMode::CurrentFight;
                    self.selected_encounter_ids.clear(); // Clear encounter selections when switching to Current Fight
                }
                if ui.add_sized([100.0, 20.0], egui::Button::new("Overall Stats").selected(showing_overall_stats)).clicked() {
                    self.view_mode = ViewMode::OverallStats;
                    self.selected_encounter_ids.clear(); // Clear encounter selections when switching to Overall Stats
                }
                let encounters_button_text = if !self.selected_encounter_ids.is_empty() {
                    format!("Encounters ({})", self.selected_encounter_ids.len())
                } else {
                    "Encounters".to_string()
                };
                if ui.add_sized([120.0, 20.0], egui::Button::new(encounters_button_text).selected(showing_encounters || self.view_mode == ViewMode::MultipleSelected)).clicked() {
                    if self.view_mode == ViewMode::MultipleSelected {
                        // If already in multi-select mode, close the selection UI but keep showing the data
                        self.view_mode = ViewMode::CurrentFight; // This will be overridden by get_current_stats if encounters are selected
                    } else {
                        // Switch to multi-select mode to show the selection UI
                        self.view_mode = ViewMode::MultipleSelected;
                    }
                }
            });
            
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
            
            // Simple resize grip using a different approach
            let window_rect = ui.max_rect();
            let grip_size = 10.0;
            
            // Bottom-right corner resize grip only
            let grip_rect = egui::Rect::from_min_size(
                egui::Pos2::new(window_rect.max.x - grip_size, window_rect.max.y - grip_size),
                egui::Vec2::new(grip_size, grip_size)
            );
            
            // Use interact instead of allocate_rect for better behavior
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
                let offset = i as f32 * 2.5;
                let start = egui::Pos2::new(grip_rect.max.x - 1.0 - offset, grip_rect.min.y + offset + 1.0);
                let end = egui::Pos2::new(grip_rect.min.x + offset + 1.0, grip_rect.max.y - 1.0 - offset);
                painter.line_segment([start, end], egui::Stroke::new(1.0, grip_color));
            }
            
        });
    }
    
    /// Keep window background opaque and visible.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // Use egui's panel background color for consistent appearance across platforms
        let color = visuals.panel_fill;
        [
            color.r() as f32 / 255.0,
            color.g() as f32 / 255.0, 
            color.b() as f32 / 255.0,
            color.a() as f32 / 255.0
        ]
    }
}