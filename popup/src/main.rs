use anyhow::Result;
use clap::Parser;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};

const APP_ID: &str = "org.dictation.Popup";

#[derive(Parser)]
#[command(name = "dictation-popup")]
#[command(about = "Display transcription results")]
struct Args {
    #[arg(short, long)]
    text: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(move |app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Dictation Result")
            .default_width(400)
            .default_height(200)
            .build();

        // TODO: Create text display widget
        // TODO: Add copy/discard buttons
        // TODO: Handle keyboard shortcuts
        
        window.present();
    });

    app.run();
    Ok(())
}