use eframe::egui;
use crate::config::{Config, load_config, save_config, UIConfig, WhisperConfig};

pub struct SettingsApp {
    config: Config,
    model: String,
    timeout: f32,
    language: String,
    fuzzy_match_threshold: f32,
    auto_copy: bool,
    auto_close_after_copy: bool,
}

impl SettingsApp {
    pub fn new() -> Self {
        let config = load_config();
        let model = config.model();
        let timeout = config.model_timeout_seconds() as f32;
        let language = config.language();
        let fuzzy_match_threshold = config.fuzzy_match_threshold();
        let auto_copy = config.auto_copy();
        let auto_close_after_copy = config.auto_close_after_copy();

        Self {
            config,
            model,
            timeout,
            language,
            fuzzy_match_threshold,
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

            // Model settings
            ui.label("Model Configuration:");
            ui.horizontal(|ui| {
                ui.label("Model:");
                ui.text_edit_singleline(&mut self.model);
            });

            ui.horizontal(|ui| {
                ui.label("Timeout (seconds):");
                ui.add(egui::DragValue::new(&mut self.timeout).range(60.0..=3600.0));
            });

            ui.horizontal(|ui| {
                ui.label("Language:");
                ui.text_edit_singleline(&mut self.language);
            });

            ui.horizontal(|ui| {
                ui.label("Fuzzy match threshold:");
                ui.add(egui::Slider::new(&mut self.fuzzy_match_threshold, 0.5..=1.0).text("similarity"));
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // UI settings
            ui.label("UI Settings:");
            ui.checkbox(&mut self.auto_copy, "Auto-copy transcript when recording completes");
            ui.add_space(5.0);

            ui.checkbox(&mut self.auto_close_after_copy, "Auto-close window after copying");

            ui.add_space(30.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.add_sized([100.0, 35.0], egui::Button::new("Save")).clicked() {
                    // Update config and save
                    let mut new_config = self.config.clone();

                    // Update whisper config
                    if new_config.whisper.is_none() {
                        new_config.whisper = Some(WhisperConfig {
                            model: Some(self.model.clone()),
                            model_timeout_seconds: Some(self.timeout as u32),
                            language: Some(self.language.clone()),
                            fuzzy_match_threshold: Some(self.fuzzy_match_threshold),
                        });
                    } else {
                        let whisper_config = new_config.whisper.as_mut().unwrap();
                        whisper_config.model = Some(self.model.clone());
                        whisper_config.model_timeout_seconds = Some(self.timeout as u32);
                        whisper_config.language = Some(self.language.clone());
                        whisper_config.fuzzy_match_threshold = Some(self.fuzzy_match_threshold);
                    }

                    // Update UI config
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

                            // Tell daemon to reload config
                            std::thread::spawn(|| {
                                let rt = tokio::runtime::Runtime::new().unwrap();
                                rt.block_on(async {
                                    if let Err(e) = crate::daemon_comm::send_reload_config().await {
                                        log::error!("Failed to send reload config: {}", e);
                                    }
                                });
                            });

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