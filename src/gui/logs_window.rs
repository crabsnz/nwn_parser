use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::models::AppSettings;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub content: String,
    pub log_type: LogType,
}

/// Structure to accumulate damage immunity absorptions for the same target at the same timestamp
#[derive(Debug, Clone)]
pub struct DamageImmunityAccumulator {
    pub timestamp: String,
    pub target: String,
    pub absorptions: Vec<(u32, String)>, // (amount, damage_type)
    pub resistance_total: u32, // Total from Damage Resistance/Reduction (no type specified)
    pub is_attack_immunity: bool, // True if these immunities are from attacks (not spells)
}

impl DamageImmunityAccumulator {
    pub fn to_log_entry(&self) -> LogEntry {
        let mut parts = Vec::new();
        for (amount, dtype) in &self.absorptions {
            // Normalize damage type names - remove "Energy" suffix for display
            let display_type = dtype.replace(" Energy", "");
            parts.push(format!("{} {}", amount, display_type));
        }
        let content = format!("{} : Damage Immunity absorbs {}", self.target, parts.join(", "));

        LogEntry {
            timestamp: self.timestamp.clone(),
            content,
            log_type: LogType::CombatDamage,
        }
    }

    /// Format absorptions as a string for appending to damage lines: "absorbs: N (X Type1 Y Type2), resisted: M"
    pub fn format_absorption_suffix(&self) -> String {
        use std::collections::HashMap;

        let mut result = String::new();

        // Format typed absorptions (Damage Immunity)
        if !self.absorptions.is_empty() {
            // Aggregate by damage type
            let mut type_totals: HashMap<String, u32> = HashMap::new();

            for (amount, dtype) in &self.absorptions {
                *type_totals.entry(dtype.clone()).or_insert(0) += amount;
            }

            // Calculate total and format parts
            let total: u32 = type_totals.values().sum();
            let mut parts: Vec<String> = type_totals.iter()
                .map(|(dtype, amount)| format!("{} {}", amount, dtype))
                .collect();
            parts.sort(); // Sort for consistent ordering

            result.push_str(&format!(", absorbs: {} ({})", total, parts.join(" ")));
        }

        // Format resistance total (Damage Resistance/Reduction)
        if self.resistance_total > 0 {
            result.push_str(&format!(", resisted: {}", self.resistance_total));
        }

        result
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogType {
    Chat,
    CombatRoll,
    CombatDamage,
    CombatOther,
    SpellCast,
    BuffExpiration,
    Other,
}

impl LogType {
    pub fn from_content(content: &str) -> Self {
        if content.contains("has joined as a player") || content.contains("has left as a player") || content.contains("has joined the party") || content.contains("wore off") || content.contains("has worn off") {
            LogType::Other  // Join/Leave and buff expiration messages
        } else if content.contains("SPELL RESIST:") {
            LogType::SpellCast  // Spell resist checks
        } else if content.contains("Initiative Roll :") || content.contains("SAVE:") || content.contains("Healed") && content.contains("hit points") || content.contains("Immune to Critical Hits") || content.contains("You triggered a Trap!") {
            LogType::CombatOther  // Initiative rolls, saves, healing, immunity messages, traps
        } else if content.contains("attacks") || content.contains("*hit*") || content.contains("*miss*") || content.contains("*critical hit*") {
            LogType::CombatRoll  // Attack rolls (hit/miss/critical)
        } else if content.contains("damages") || content.contains("Damage Immunity") {
            LogType::CombatDamage  // Damage messages
        } else if content.contains("casts") || content.contains("casting") {
            LogType::SpellCast
        } else if content.to_lowercase().contains("[talk]") || content.to_lowercase().contains("[tell]") || content.to_lowercase().contains("[party]") || content.to_lowercase().contains("[shout]") || content.to_lowercase().contains("[say]") || content.to_lowercase().contains("[whisper]") || content.to_lowercase().contains("[server]") || content.to_lowercase().contains("[dm]") {
            LogType::Chat
        } else {
            LogType::Other
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            LogType::Chat => "Chat",
            LogType::CombatRoll => "Combat Rolls",
            LogType::CombatDamage => "Combat Damage",
            LogType::CombatOther => "Combat - Other",
            LogType::SpellCast => "Spell Casting",
            LogType::BuffExpiration => "Buff Expiration",
            LogType::Other => "Other",
        }
    }
}

pub struct LogsWindowState {
    pub recent_logs: Arc<Mutex<Vec<LogEntry>>>,
    pub show_chat: bool,
    pub show_combat_rolls: bool,
    pub show_combat_damage: bool,
    pub show_combat_other: bool,
    pub show_spell_cast: bool,
    pub show_other: bool,
    pub scroll_to_bottom: bool,
    pub show_timestamps: bool,
    pub last_scroll_offset: f32,
    pub search_text: String,
    pub filters_popup_open: bool,
    pub filters_button_rect: Option<egui::Rect>,
}

impl Default for LogsWindowState {
    fn default() -> Self {
        Self {
            recent_logs: Arc::new(Mutex::new(Vec::new())),
            show_chat: true,
            show_combat_rolls: true,
            show_combat_damage: true,
            show_combat_other: true,
            show_spell_cast: true,
            show_other: false,
            scroll_to_bottom: true,
            show_timestamps: true,
            last_scroll_offset: 0.0,
            search_text: String::new(),
            filters_popup_open: false,
            filters_button_rect: None,
        }
    }
}

impl LogsWindowState {
    pub fn add_log_entry(&self, timestamp: String, content: String) {
        let log_type = LogType::from_content(&content);
        let entry = LogEntry {
            timestamp,
            content,
            log_type,
        };

        if let Ok(mut logs) = self.recent_logs.lock() {
            logs.push(entry);
        }
    }

    pub fn get_filtered_logs(&self) -> Vec<LogEntry> {
        if let Ok(logs) = self.recent_logs.lock() {
            let search_lower = self.search_text.to_lowercase();
            logs.iter()
                .filter(|entry| {
                    // Filter by type
                    let type_match = match entry.log_type {
                        LogType::Chat => self.show_chat,
                        LogType::CombatRoll => self.show_combat_rolls,
                        LogType::CombatDamage => self.show_combat_damage,
                        LogType::CombatOther => self.show_combat_other,
                        LogType::SpellCast => self.show_spell_cast,
                        LogType::BuffExpiration => self.show_other, // Mapped to Other
                        LogType::Other => self.show_other,
                    };

                    // Filter by search text (if any)
                    let search_match = if search_lower.is_empty() {
                        true
                    } else {
                        entry.content.to_lowercase().contains(&search_lower)
                    };

                    type_match && search_match
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get color for log entry based on content and type
    fn get_log_color(content: &str, log_type: &LogType) -> egui::Color32 {
        // Check for join/leave messages first (grey)
        if content.contains("has joined as a player") || content.contains("has left as a player") || content.contains("has joined the party") {
            return egui::Color32::GRAY;
        }

        // Check for item acquisition/loss messages (yellow)
        if content.contains("You have acquired") || content.contains("You have lost") || content.contains("picks up") || content.contains("drops") {
            return egui::Color32::from_rgb(255, 255, 0); // Yellow
        }

        // Check for specific chat types based on content (case-insensitive)
        let content_lower = content.to_lowercase();
        if content_lower.contains("[tell]") {
            return egui::Color32::from_rgb(32, 255, 32); // TellColor
        }
        if content_lower.contains("[talk]") {
            return egui::Color32::from_rgb(240, 240, 240); // TalkColor
        }
        if content_lower.contains("[party]") {
            return egui::Color32::from_rgb(255, 102, 1); // PartyColor
        }
        if content_lower.contains("[shout]") {
            return egui::Color32::from_rgb(255, 239, 80); // ShoutColor
        }
        if content_lower.contains("[whisper]") {
            return egui::Color32::from_rgb(128, 128, 128); // WhisperColor
        }
        if content_lower.contains("[server]") {
            return egui::Color32::from_rgb(176, 176, 176); // ServerColor
        }
        if content_lower.contains("[dm]") {
            return egui::Color32::from_rgb(16, 223, 255); // DMColor
        }

        // Apply colors based on log type
        match log_type {
            LogType::Chat => egui::Color32::from_rgb(0, 128, 128), // Teal for general chat
            LogType::CombatRoll | LogType::CombatDamage => egui::Color32::from_rgb(255, 165, 0), // Orange for combat
            LogType::CombatOther => egui::Color32::from_rgb(135, 206, 250), // Sky blue for combat other
            LogType::SpellCast | LogType::BuffExpiration => egui::Color32::from_rgb(128, 0, 128), // Purple for casting and buffs
            LogType::Other => egui::Color32::WHITE,
        }
    }

    /// Get color for damage types
    fn get_damage_type_color(damage_type: &str) -> egui::Color32 {
        match damage_type.to_lowercase().as_str() {
            "physical" => egui::Color32::from_rgb(220, 220, 220),
            "magical" => egui::Color32::from_rgb(147, 112, 219),
            "divine" => egui::Color32::from_rgb(255, 215, 0),
            "negative" | "negative energy" => egui::Color32::from_rgb(128, 128, 128),
            "positive" | "positive energy" => egui::Color32::from_rgb(255, 255, 255),
            "acid" => egui::Color32::from_rgb(34, 139, 34),
            "pure" => egui::Color32::from_rgb(255, 20, 147),
            "cold" => egui::Color32::from_rgb(135, 206, 250),
            "sonic" => egui::Color32::from_rgb(255, 200, 124),
            "fire" => egui::Color32::from_rgb(255, 69, 0),
            "electrical" => egui::Color32::from_rgb(255, 255, 0),
            _ => egui::Color32::WHITE,
        }
    }

    /// Extract character name from log line
    fn extract_character_name(content: &str) -> Option<String> {
        // For lines like "Name casts Spell" or "Name attacks Target"
        if let Some(pos) = content.find(" casts ") {
            return Some(content[..pos].trim().to_string());
        }
        if let Some(pos) = content.find(" casting ") {
            return Some(content[..pos].trim().to_string());
        }
        if let Some(pos) = content.find(" attacks ") {
            return Some(content[..pos].trim().to_string());
        }
        if let Some(pos) = content.find(" damages ") {
            return Some(content[..pos].trim().to_string());
        }
        // For lines like "Name: [Shout] message" or "Name : [Party] message"
        if let Some(pos) = content.find(": [") {
            return Some(content[..pos].trim().to_string());
        }
        // For lines like "Name : Initiative Roll :" or "Name : Healed X hit points"
        if let Some(pos) = content.find(" : ") {
            let potential_name = content[..pos].trim();
            // Make sure it's not a target after an attacker
            if !potential_name.contains(" attacks ") && !potential_name.contains(" damages ") {
                return Some(potential_name.to_string());
            }
        }
        None
    }

    /// Render log content with rich text (colored names and damage types)
    fn render_rich_log_content(ui: &mut egui::Ui, content: &str, log_type: &LogType, base_color: egui::Color32, show_timestamp: bool, timestamp: &str) {
        use egui::RichText;

        // Special handling for SAVE lines - color everything after the name in sky blue
        if content.contains("SAVE:") {
            ui.horizontal_wrapped(|ui| {
                if show_timestamp {
                    ui.label(RichText::new(timestamp).color(egui::Color32::GRAY));
                }

                ui.spacing_mut().item_spacing.x = 0.0;

                // Split at the first " : " after "SAVE:"
                if let Some(save_pos) = content.find("SAVE:") {
                    let after_save = &content[save_pos + 5..].trim_start();
                    if let Some(colon_pos) = after_save.find(" : ") {
                        let name_part = &after_save[..colon_pos];
                        let rest_part = &after_save[colon_pos..];

                        // Render "SAVE: " in default color
                        ui.label(RichText::new("SAVE: ").color(base_color));
                        // Render name in red
                        ui.label(RichText::new(name_part).color(egui::Color32::from_rgb(255, 0, 0)));
                        // Render rest in sky blue
                        ui.label(RichText::new(rest_part).color(egui::Color32::from_rgb(135, 206, 250)));
                    } else {
                        ui.label(RichText::new(content).color(base_color));
                    }
                } else {
                    ui.label(RichText::new(content).color(base_color));
                }
            });
            return;
        }

        // Extract character name if present
        let character_name = Self::extract_character_name(content);

        // Split content by damage types to colorize them
        let damage_types = ["Physical", "Magical", "Divine", "Negative Energy", "Positive Energy",
                           "Acid", "Pure", "Cold", "Sonic", "Fire", "Electrical"];

        let mut remaining = content;
        let mut segments: Vec<(String, Option<egui::Color32>)> = Vec::new();

        // First, handle character name coloring
        if let Some(ref name) = character_name {
            if let Some(name_pos) = remaining.find(name.as_str()) {
                // Add text before name
                if name_pos > 0 {
                    segments.push((remaining[..name_pos].to_string(), None));
                }
                // Add name in red
                segments.push((name.clone(), Some(egui::Color32::from_rgb(255, 0, 0))));
                // Continue with rest of content
                remaining = &remaining[name_pos + name.len()..];
            }
        }

        // Now process remaining content for damage types AND numbers before damage types
        let mut temp_segments: Vec<(String, Option<egui::Color32>)> = Vec::new();
        let mut current_text = remaining.to_string();

        while !current_text.is_empty() {
            let mut found_damage_type = None;
            let mut earliest_pos = current_text.len();

            // Find the earliest occurrence of any damage type
            for dtype in &damage_types {
                if let Some(pos) = current_text.find(dtype) {
                    if pos < earliest_pos {
                        earliest_pos = pos;
                        found_damage_type = Some(*dtype);
                    }
                }
            }

            if let Some(dtype) = found_damage_type {
                // Check if there's a number immediately before the damage type
                let text_before = &current_text[..earliest_pos];

                // Look for a number at the end of text_before (e.g., "39 " before "Physical")
                let mut number_start = text_before.len();
                let mut found_number = false;

                // Scan backwards from the end to find where the number starts
                let chars: Vec<char> = text_before.chars().collect();
                let mut i = chars.len();

                // Skip trailing whitespace
                while i > 0 && chars[i - 1].is_whitespace() {
                    i -= 1;
                }

                // Now collect digits
                let number_end = i;
                while i > 0 && chars[i - 1].is_numeric() {
                    i -= 1;
                    found_number = true;
                }

                if found_number {
                    number_start = i;

                    // Add text before the number (if any)
                    if number_start > 0 {
                        let before_number: String = chars[..number_start].iter().collect();
                        temp_segments.push((before_number, None));
                    }

                    // Add the number with damage type color
                    let number_text: String = chars[number_start..number_end].iter().collect();
                    temp_segments.push((number_text, Some(Self::get_damage_type_color(dtype))));

                    // Add whitespace between number and damage type (if any)
                    if number_end < text_before.len() {
                        let whitespace: String = chars[number_end..].iter().collect();
                        temp_segments.push((whitespace, None));
                    }
                } else {
                    // No number found, add text before damage type
                    if earliest_pos > 0 {
                        temp_segments.push((text_before.to_string(), None));
                    }
                }

                // Add damage type with its color
                temp_segments.push((dtype.to_string(), Some(Self::get_damage_type_color(dtype))));
                // Continue with rest
                current_text = current_text[earliest_pos + dtype.len()..].to_string();
            } else {
                // No more damage types found
                if !current_text.is_empty() {
                    temp_segments.push((current_text.clone(), None));
                }
                break;
            }
        }

        // If we had a character name, append the damage-type-processed segments
        // Otherwise, use them directly
        if character_name.is_some() {
            segments.extend(temp_segments);
        } else {
            segments = temp_segments;
        }

        // Render all segments (including timestamp if requested)
        ui.horizontal_wrapped(|ui| {
            // Add timestamp first if enabled
            if show_timestamp {
                ui.label(RichText::new(timestamp).color(egui::Color32::GRAY));
            }

            // Disable spacing between labels to prevent extra gaps between colored text
            ui.spacing_mut().item_spacing.x = 0.0;

            for (text, color_override) in segments {
                if let Some(color) = color_override {
                    ui.label(RichText::new(text).color(color));
                } else {
                    ui.label(RichText::new(text).color(base_color));
                }
            }
        });
    }
}

/// Show the logs window as a separate viewport (independent window)
pub fn show_logs_window(
    ctx: &egui::Context,
    logs_state: &mut LogsWindowState,
    _settings: Arc<Mutex<AppSettings>>,
    is_open: &mut bool
) {
    if !*is_open {
        return;
    }

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("logs_window"),
        egui::ViewportBuilder::default()
            .with_inner_size([600.0, 400.0])
            .with_min_inner_size([400.0, 300.0])
            .with_max_inner_size([1600.0, 1200.0])
            .with_resizable(true)
            .with_decorations(false)  // Remove system decorations for custom title bar
            .with_always_on_top()
            .with_title("Logs"),
        |ctx, class| {
            assert!(class == egui::ViewportClass::Immediate);
            ctx.set_visuals(egui::Visuals::dark());

            egui::CentralPanel::default().show(ctx, |ui| {
                // Custom header bar (like main window)
                let header_rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), 35.0)).1;

                // Make the header draggable except for the button areas
                let draggable_rect = egui::Rect::from_min_size(
                    header_rect.min,
                    egui::Vec2::new(header_rect.width() - 60.0, header_rect.height()) // Leave space for X and minimize buttons
                );
                let drag_response = ui.allocate_rect(draggable_rect, egui::Sense::click_and_drag());
                if drag_response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // Draw the header content
                ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                    ui.horizontal(|ui| {
                        // Left-aligned title
                        let title_pos = egui::Pos2::new(header_rect.min.x + 15.0, header_rect.center().y);
                        ui.painter().text(title_pos, egui::Align2::LEFT_CENTER, "Logs",
                            egui::FontId::proportional(16.0), ui.visuals().text_color());

                        // Push buttons to the right
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Close button (X) - sets window closed
                            if ui.add(egui::Button::new(egui::RichText::new("X").size(12.0))
                                .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                                *is_open = false;
                            }

                            // Minimize button
                            if ui.add(egui::Button::new(egui::RichText::new("−").size(12.0))
                                .min_size(egui::Vec2::new(25.0, 25.0))).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                            }
                        });
                    });
                });

                ui.separator();

                // Search and filter controls
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    let search_response = ui.add(egui::TextEdit::singleline(&mut logs_state.search_text)
                        .hint_text("Filter by text...")
                        .desired_width(200.0));

                    // Clear search button (X)
                    if !logs_state.search_text.is_empty() && ui.small_button("✖").clicked() {
                        logs_state.search_text.clear();
                    }

                    ui.separator();

                    // Filters dropdown menu
                    let filters_button = ui.button("Filters");
                    logs_state.filters_button_rect = Some(filters_button.rect);
                    if filters_button.clicked() {
                        logs_state.filters_popup_open = !logs_state.filters_popup_open;
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        if let Ok(mut logs) = logs_state.recent_logs.lock() {
                            logs.clear();
                        }
                    }

                    ui.separator();

                    // Timestamp toggle
                    ui.checkbox(&mut logs_state.show_timestamps, "Timestamps");

                    // Auto-scroll toggle
                    ui.checkbox(&mut logs_state.scroll_to_bottom, "Auto-scroll");
                });

                ui.separator();

                // Main content area with virtualized rendering
                let text_height = ui.text_style_height(&egui::TextStyle::Body);
                let scroll_area = egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(logs_state.scroll_to_bottom);

                scroll_area.show(ui, |ui| {
                    // Show logs with unified filtering
                    let filtered_logs = logs_state.get_filtered_logs();

                    if filtered_logs.is_empty() {
                        ui.centered_and_justified(|ui| {
                            // Check if there are any logs at all vs just filtered out
                            let total_logs = if let Ok(logs) = logs_state.recent_logs.lock() {
                                logs.len()
                            } else {
                                0
                            };

                            if total_logs == 0 {
                                ui.label("No logs available - waiting for game logs...");
                            } else {
                                ui.label("No logs match the current filter");
                            }
                        });
                    } else {
                        // Virtualized rendering for performance
                        let total_entries = filtered_logs.len();

                        // Estimate line height (account for wrapping - use 2x text height as estimate)
                        let estimated_line_height = text_height * 2.0;

                        // Get available height
                        let available_height = ui.available_height();

                        // Calculate visible range with buffer
                        let buffer_entries = 50; // Render extra entries above/below for smooth scrolling
                        let visible_entries = (available_height / estimated_line_height).ceil() as usize;

                        // Calculate scroll offset
                        let scroll_offset = ui.clip_rect().min.y - ui.min_rect().min.y;
                        let first_visible_index = ((scroll_offset.max(0.0)) / estimated_line_height).floor() as usize;

                        // Calculate range to render (clamp to valid bounds)
                        let start_index = first_visible_index.saturating_sub(buffer_entries).min(total_entries);
                        let end_index = (first_visible_index + visible_entries + buffer_entries).min(total_entries);

                        // Only render if we have a valid range (prevents crash when filter reduces entries)
                        if start_index < end_index {
                            // Add spacing for entries before visible range
                            if start_index > 0 {
                                let skip_height = start_index as f32 * estimated_line_height;
                                ui.add_space(skip_height);
                            }

                            // Render only visible entries
                            for entry in &filtered_logs[start_index..end_index] {
                                let base_color = LogsWindowState::get_log_color(&entry.content, &entry.log_type);
                                LogsWindowState::render_rich_log_content(ui, &entry.content, &entry.log_type, base_color, logs_state.show_timestamps, &entry.timestamp);
                            }

                            // Add spacing for entries after visible range
                            if end_index < total_entries {
                                let skip_height = (total_entries - end_index) as f32 * estimated_line_height;
                                ui.add_space(skip_height);
                            }
                        }
                    }
                });
            }); // End CentralPanel

            // Show filters popup if open
            if logs_state.filters_popup_open {
                if let Some(button_rect) = logs_state.filters_button_rect {
                    let popup_pos = button_rect.left_bottom() + egui::vec2(0.0, 5.0);
                    egui::Area::new(egui::Id::new("filters_popup_area"))
                        .fixed_pos(popup_pos)
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.set_min_width(150.0);
                                ui.checkbox(&mut logs_state.show_chat, "Chat");
                                ui.checkbox(&mut logs_state.show_combat_rolls, "Combat Rolls");
                                ui.checkbox(&mut logs_state.show_combat_damage, "Combat Damage");
                                ui.checkbox(&mut logs_state.show_combat_other, "Combat - Other");
                                ui.checkbox(&mut logs_state.show_spell_cast, "Spell Casting");
                                ui.checkbox(&mut logs_state.show_other, "Other");
                            });
                        });

                    // Close if clicked outside the popup
                    if ctx.input(|i| i.pointer.any_click()) {
                        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
                        if let Some(pos) = pointer_pos {
                            // Check if click is outside both the button and popup area
                            let popup_rect = egui::Rect::from_min_size(popup_pos, egui::vec2(150.0, 150.0));
                            if !button_rect.contains(pos) && !popup_rect.contains(pos) {
                                logs_state.filters_popup_open = false;
                            }
                        }
                    }
                }
            }

            // Render resize grip OUTSIDE the panel to ensure it's truly on top
            let window_rect = ctx.screen_rect();
            let grip_size = 15.0; // Slightly larger for easier grabbing

            // Bottom-right corner resize grip only
            let grip_rect = egui::Rect::from_min_size(
                egui::Pos2::new(window_rect.max.x - grip_size, window_rect.max.y - grip_size),
                egui::Vec2::new(grip_size, grip_size)
            );

            // Create a top-layer area for the resize grip to ensure it's above everything
            egui::Area::new(egui::Id::new("logs_resize_grip_area"))
                .fixed_pos(grip_rect.min)
                .interactable(true)
                .show(ctx, |ui| {
                    let grip_id = egui::Id::new("logs_resize_grip");
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

                            let new_width = (current_size.x + delta.x).clamp(400.0, 1600.0);
                            let new_height = (current_size.y + delta.y).clamp(300.0, 1200.0);

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
        },
    );
}

