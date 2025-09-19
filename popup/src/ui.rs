use eframe::egui;
use crate::app::DictationApp;

impl eframe::App for DictationApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process any pending messages from daemon thread
        self.process_messages();

        // Request repaint to keep UI responsive
        ctx.request_repaint();

        // Settings window (modal)
        if self.show_settings {
            show_settings_window(ctx, self);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Set larger font sizes
            let mut style = (*ctx.style()).clone();
            style.text_styles.insert(
                egui::TextStyle::Body,
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Heading,
                egui::FontId::new(20.0, egui::FontFamily::Proportional),
            );
            ctx.set_style(style);

            // Header with title and settings
            ui.horizontal(|ui| {
                ui.heading("Voice Dictation");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("⚙").clicked() {
                        self.show_settings = true;
                    }
                });
            });

            // Status indicator (3 states only)
            ui.horizontal(|ui| {
                if self.model_loading {
                    ui.colored_label(egui::Color32::YELLOW, "Loading model...");
                } else if self.is_recording {
                    // Pulsing red circle
                    let time = ctx.input(|i| i.time) as f32;
                    let pulse = (time * 3.0).sin() * 0.3 + 0.7; // Pulse between 0.4 and 1.0
                    let red_component = (255.0 * pulse) as u8;
                    let pulsing_red = egui::Color32::from_rgb(red_component, 0, 0);

                    // Draw a filled circle instead of text
                    let (rect, _response) = ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 6.0, pulsing_red);
                    ui.label("Recording");
                } else {
                    let (rect, _response) = ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 6.0, egui::Color32::GREEN);
                    ui.label("Done");
                }
            });

            ui.separator();

            // Text area
            ui.label("Transcription:");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    if self.is_recording {
                        // Read-only display during recording to prevent flashing
                        ui.add_sized(
                            [ui.available_width(), ui.available_height()],
                            egui::TextEdit::multiline(&mut self.text.clone())
                                .font(egui::TextStyle::Body)
                                .interactive(false)
                        );
                    } else {
                        // Editable after recording
                        ui.add_sized(
                            [ui.available_width(), ui.available_height()],
                            egui::TextEdit::multiline(&mut self.text)
                                .font(egui::TextStyle::Body)
                        );
                    }
                });

            ui.separator();

            // Buttons
            ui.horizontal(|ui| {
                if self.is_recording {
                    if ui.add_sized([120.0, 40.0], egui::Button::new("⏹ Stop Recording")).clicked() {
                        self.stop_recording();
                    }
                } else {
                    if ui.add_sized([100.0, 40.0], egui::Button::new("📋 Copy")).clicked() {
                        self.copy_to_clipboard();
                    }

                    if ui.add_sized([100.0, 40.0], egui::Button::new("🗑 Discard")).clicked() {
                        std::process::exit(0);
                    }
                }
            });

            // Keyboard shortcuts
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !self.is_recording {
                self.copy_to_clipboard();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                std::process::exit(0);
            }
        });
    }
}

fn show_settings_window(ctx: &egui::Context, app: &mut DictationApp) {
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(300.0);

            ui.heading("Voice Dictation Settings");
            ui.separator();

            // Create mutable copies for the UI
            let mut auto_copy = app.config.auto_copy();
            let mut auto_close_after_copy = app.config.auto_close_after_copy();

            ui.checkbox(&mut auto_copy, "Auto-copy transcript when recording completes");
            ui.add_space(5.0);
            ui.checkbox(&mut auto_close_after_copy, "Auto-close window after copying");

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    // Update config and save
                    let mut new_config = app.config.clone();
                    if new_config.ui.is_none() {
                        new_config.ui = Some(crate::config::UIConfig {
                            auto_copy: Some(auto_copy),
                            auto_close_after_copy: Some(auto_close_after_copy),
                        });
                    } else {
                        let ui_config = new_config.ui.as_mut().unwrap();
                        ui_config.auto_copy = Some(auto_copy);
                        ui_config.auto_close_after_copy = Some(auto_close_after_copy);
                    }

                    if let Err(e) = crate::config::save_config(&new_config) {
                        log::error!("Failed to save config: {}", e);
                    } else {
                        app.config = new_config;
                        log::info!("Settings saved successfully");
                    }

                    app.show_settings = false;
                }

                if ui.button("Cancel").clicked() {
                    app.show_settings = false;
                }
            });
        });
}