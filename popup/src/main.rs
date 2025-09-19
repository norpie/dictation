use anyhow::Result;
use clap::Parser;
use eframe::egui;

mod app;
mod config;
mod daemon_comm;
mod ui;
mod settings_app;

use app::DictationApp;
use settings_app::SettingsApp;

#[derive(Parser)]
#[command(name = "dictation-popup")]
#[command(about = "Voice dictation interface")]
struct Args {
    #[arg(short, long)]
    text: Option<String>,

    #[arg(short, long)]
    settings: bool,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    if args.settings {
        // Launch settings window
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([500.0, 450.0])
                .with_title("Voice Dictation Settings")
                .with_resizable(true),
            ..Default::default()
        };

        eframe::run_native(
            "Voice Dictation Settings",
            options,
            Box::new(|_cc| Ok(Box::new(SettingsApp::new()))),
        ).map_err(|e| anyhow::anyhow!("Failed to run settings app: {}", e))
    } else {
        // Launch main dictation window
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([600.0, 400.0])
                .with_title("Voice Dictation"),
            ..Default::default()
        };

        eframe::run_native(
            "Voice Dictation",
            options,
            Box::new(|_cc| Ok(Box::new(DictationApp::new(args.text)))),
        ).map_err(|e| anyhow::anyhow!("Failed to run egui app: {}", e))
    }
}