use anyhow::Result;
use clap::Parser;
use log::info;

#[derive(Parser)]
#[command(name = "dictation-client")]
#[command(about = "Trigger dictation recording")]
struct Args {
    #[arg(short, long)]
    start: bool,
    
    #[arg(short, long)]
    stop: bool,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    
    info!("Dictation client started");
    
    // TODO: Connect to daemon via IPC
    // TODO: Send start/stop commands
    // TODO: Stream audio to daemon
    
    Ok(())
}