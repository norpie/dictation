use eframe::egui;
use crate::config::{Config, load_config, save_config, UIConfig};

pub struct SettingsApp {
    config: Config,
    auto_copy: bool,
    auto_close_after_copy: bool,
}

impl SettingsApp {
    pub fn new() -> Self {
        let config = load_config();
        let auto_copy = config.auto_copy();
        let auto_close_after_copy = config.auto_close_after_copy();

        Self {
            config,
            auto_copy,
            auto_close_after_copy,
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

            ui.heading("Voice Dictation Settings");
            ui.separator();

            ui.add_space(20.0);

            ui.checkbox(&mut self.auto_copy, "Auto-copy transcript when recording completes");
            ui.add_space(10.0);

            ui.checkbox(&mut self.auto_close_after_copy, "Auto-close window after copying");

            ui.add_space(30.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.add_sized([100.0, 35.0], egui::Button::new("Save")).clicked() {
                    // Update config and save
                    let mut new_config = self.config.clone();
                    if new_config.ui.is_none() {
                        new_config.ui = Some(UIConfig {
                            auto_copy: Some(self.auto_copy),
                            auto_close_after_copy: Some(self.auto_close_after_copy),
                        });
                    } else {
                        let ui_config = new_config.ui.as_mut().unwrap();
                        ui_config.auto_copy = Some(self.auto_copy);
                        ui_config.auto_close_after_copy = Some(self.auto_close_after_copy);
                    }

                    match save_config(&new_config) {
                        Ok(_) => {
                            // Show success notification
                            let _ = std::process::Command::new("notify-send")
                                .arg("Voice Dictation")
                                .arg("Settings saved successfully")
                                .arg("--expire-time=2000")
                                .spawn();

                            self.config = new_config;
                            log::info!("Settings saved successfully");
                            std::process::exit(0);
                        }
                        Err(e) => {
                            log::error!("Failed to save config: {}", e);
                            // Show error notification
                            let _ = std::process::Command::new("notify-send")
                                .arg("Voice Dictation")
                                .arg(&format!("Failed to save settings: {}", e))
                                .arg("--urgency=critical")
                                .spawn();
                        }
                    }
                }

                ui.add_space(10.0);

                if ui.add_sized([100.0, 35.0], egui::Button::new("Cancel")).clicked() {
                    std::process::exit(0);
                }
            });

            // Keyboard shortcuts
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                std::process::exit(0);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                // Trigger save
                // (Save logic would be duplicated here - could be extracted to a method)
            }
        });
    }
}