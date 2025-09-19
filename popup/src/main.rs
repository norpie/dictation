use anyhow::Result;
use clap::Parser;
use eframe::egui;
use shared::ipc::{ClientMessage, DaemonMessage, protocol};
use std::sync::mpsc;
use tokio::net::UnixStream;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "dictation-popup")]
#[command(about = "Voice dictation interface")]
struct Args {
    #[arg(short, long)]
    text: Option<String>,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 300.0])
            .with_title("Voice Dictation"),
        ..Default::default()
    };

    eframe::run_native(
        "Voice Dictation",
        options,
        Box::new(|_cc| Ok(Box::new(DictationApp::new(args.text)))),
    ).map_err(|e| anyhow::anyhow!("Failed to run egui app: {}", e))
}

struct DictationApp {
    text: String,
    complete_text: String,
    recording_status: String,
    daemon_connected: bool,
    is_recording: bool,
    session_id: Option<Uuid>,
    rx: mpsc::Receiver<UiMessage>,
    _tx: mpsc::Sender<UiMessage>, // Keep sender alive
}

#[derive(Debug)]
enum UiMessage {
    DaemonConnected(bool),
    RecordingStarted(Uuid),
    RecordingStopped,
    TranscriptionUpdate(String),
    TranscriptionComplete(String),
    Error(String),
}

impl DictationApp {
    fn new(initial_text: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();

        // Start daemon communication in background thread
        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            daemon_communication_thread(tx_clone);
        });

        Self {
            text: initial_text.unwrap_or_else(|| "Starting...".to_string()),
            complete_text: String::new(),
            recording_status: "Connecting to daemon...".to_string(),
            daemon_connected: false,
            is_recording: false,
            session_id: None,
            rx,
            _tx: tx,
        }
    }

    fn process_messages(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                UiMessage::DaemonConnected(connected) => {
                    self.daemon_connected = connected;
                    if connected {
                        self.recording_status = "🔴 Recording...".to_string();
                        self.text = "".to_string();
                        self.complete_text = String::new();
                    } else {
                        self.recording_status = "Daemon not available".to_string();
                    }
                }
                UiMessage::RecordingStarted(session_id) => {
                    self.session_id = Some(session_id);
                    self.is_recording = true;
                    self.recording_status = "🔴 Recording...".to_string();
                    self.text = "".to_string();
                    self.complete_text = String::new();
                }
                UiMessage::RecordingStopped => {
                    self.is_recording = false;
                    self.recording_status = "Recording stopped".to_string();
                    // Keep the final text for copying
                }
                UiMessage::TranscriptionUpdate(new_content) => {
                    // Daemon sends incremental new content, accumulate it
                    if !self.complete_text.is_empty() && !new_content.trim().is_empty() {
                        self.complete_text.push(' ');
                    }
                    self.complete_text.push_str(&new_content);
                    self.text = self.complete_text.clone();
                    log::info!("Added: '{}' -> Full: '{}'", new_content, self.text);
                }
                UiMessage::TranscriptionComplete(final_text) => {
                    // Add to complete text and clear current partial
                    if !self.complete_text.is_empty() && !final_text.trim().is_empty() {
                        self.complete_text.push(' ');
                    }
                    self.complete_text.push_str(&final_text);
                    self.text = self.complete_text.clone();
                    self.is_recording = false;
                    self.recording_status = "Recording complete".to_string();
                    log::info!("Complete: '{}'", final_text);
                }
                UiMessage::Error(error) => {
                    self.recording_status = format!("Error: {}", error);
                }
            }
        }
    }

    fn stop_recording(&mut self) {
        if self.is_recording {
            // Immediately update UI state
            self.is_recording = false;
            self.recording_status = "Stopping...".to_string();

            // Send stop message through a new thread to avoid blocking UI
            std::thread::spawn(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let _ = send_stop_recording().await;
                });
            });
        }
    }

    fn copy_to_clipboard(&mut self) {
        if !self.text.trim().is_empty() {
            use std::io::Write;
            match std::process::Command::new("wl-copy")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn() {
                Ok(mut child) => {
                    if let Some(stdin) = child.stdin.take() {
                        let mut stdin = stdin;
                        if stdin.write_all(self.text.as_bytes()).is_ok() {
                            drop(stdin);
                            if child.wait().map(|s| s.success()).unwrap_or(false) {
                                self.recording_status = "📋 Copied to clipboard!".to_string();
                            } else {
                                self.recording_status = "Copy failed".to_string();
                            }
                        } else {
                            self.recording_status = "Copy failed".to_string();
                        }
                    } else {
                        self.recording_status = "Copy failed".to_string();
                    }
                }
                Err(e) => {
                    self.recording_status = format!("Copy failed: {}", e);
                    log::error!("Failed to copy with wl-copy: {}", e);
                }
            }
        } else {
            self.recording_status = "No text to copy".to_string();
        }
    }
}

impl eframe::App for DictationApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process any pending messages from daemon thread
        self.process_messages();

        // Request repaint to keep UI responsive
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Voice Dictation");

            // Status line with daemon connection indicator
            ui.horizontal(|ui| {
                let color = if self.daemon_connected {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                };
                ui.colored_label(color, "●");
                ui.label(if self.daemon_connected {
                    "Connected to daemon"
                } else {
                    "Daemon not available"
                });
            });

            ui.separator();

            // Recording status
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 50.0);
                ui.label(&self.recording_status);
            });

            ui.separator();

            // Text area
            ui.label("Transcription:");
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), ui.available_height()],
                        egui::TextEdit::multiline(&mut self.text)
                            .font(egui::TextStyle::Body)
                            .interactive(!self.is_recording)
                    );
                });

            ui.separator();


            // Buttons
            ui.horizontal(|ui| {
                if self.is_recording {
                    if ui.button("⏹ Stop Recording").clicked() {
                        self.stop_recording();
                    }
                } else {
                    if ui.button("📋 Copy").clicked() {
                        self.copy_to_clipboard();
                    }

                    if ui.button("🗑 Discard").clicked() {
                        std::process::exit(0);
                    }
                }
            });

            // Keyboard shortcuts
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.copy_to_clipboard();
                std::process::exit(0);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                std::process::exit(0);
            }
        });
    }
}

fn daemon_communication_thread(tx: mpsc::Sender<UiMessage>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Check daemon status
        match check_daemon_status().await {
            Ok(true) => {
                let _ = tx.send(UiMessage::DaemonConnected(true));

                // Start recording
                match send_start_recording().await {
                    Ok((session_id, stream)) => {
                        let _ = tx.send(UiMessage::RecordingStarted(session_id));

                        // Listen for daemon messages on the same connection
                        listen_for_daemon_messages(tx, session_id, stream).await;
                    }
                    Err(e) => {
                        let _ = tx.send(UiMessage::Error(format!("Failed to start recording: {}", e)));
                    }
                }
            }
            _ => {
                let _ = tx.send(UiMessage::DaemonConnected(false));
            }
        }
    });
}

async fn check_daemon_status() -> Result<bool> {
    let socket_path = "/tmp/dictation.sock";

    match UnixStream::connect(socket_path).await {
        Ok(mut stream) => {
            if protocol::send_message(&mut stream, &ClientMessage::GetStatus).await.is_ok() {
                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(2),
                    protocol::receive_message::<DaemonMessage>(&mut stream)
                ).await {
                    Ok(Ok(DaemonMessage::Status(_))) => Ok(true),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
        Err(_) => Ok(false),
    }
}

async fn send_start_recording() -> Result<(Uuid, UnixStream)> {
    let socket_path = "/tmp/dictation.sock";
    let mut stream = UnixStream::connect(socket_path).await?;

    protocol::send_message(&mut stream, &ClientMessage::StartRecording).await?;

    match protocol::receive_message::<DaemonMessage>(&mut stream).await? {
        DaemonMessage::RecordingStarted(session_id) => Ok((session_id, stream)),
        DaemonMessage::Error(error) => anyhow::bail!("Daemon error: {}", error),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

async fn send_stop_recording() -> Result<()> {
    let socket_path = "/tmp/dictation.sock";
    let mut stream = UnixStream::connect(socket_path).await?;

    protocol::send_message(&mut stream, &ClientMessage::StopRecording).await?;
    Ok(())
}

async fn listen_for_daemon_messages(tx: mpsc::Sender<UiMessage>, session_id: Uuid, mut stream: UnixStream) {
    loop {
        match protocol::receive_message::<DaemonMessage>(&mut stream).await {
            Ok(message) => {
                match message {
                    DaemonMessage::TranscriptionUpdate { session_id: msg_session_id, partial_text } => {
                        if msg_session_id == session_id {
                            let _ = tx.send(UiMessage::TranscriptionUpdate(partial_text));
                        }
                    }
                    DaemonMessage::TranscriptionComplete(session) => {
                        if session.id == session_id {
                            let _ = tx.send(UiMessage::TranscriptionComplete(session.text));
                        }
                    }
                    DaemonMessage::RecordingStopped => {
                        let _ = tx.send(UiMessage::RecordingStopped);
                        return; // Exit listen loop
                    }
                    DaemonMessage::Error(error) => {
                        let _ = tx.send(UiMessage::Error(error));
                    }
                    _ => {}
                }
            }
            Err(e) => {
                let _ = tx.send(UiMessage::Error(format!("Connection lost: {}", e)));
                return;
            }
        }
    }
}