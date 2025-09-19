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

struct DictationApp {
    text: String,
    complete_text: String,
    daemon_connected: bool,
    is_recording: bool,
    session_id: Option<Uuid>,

    // UI state
    model_loading: bool,

    rx: mpsc::Receiver<UiMessage>,
    _tx: mpsc::Sender<UiMessage>, // Keep sender alive
}

#[derive(Debug)]
enum UiMessage {
    DaemonConnected(bool),
    RecordingStarted(Uuid),
    RecordingStopped,
    TranscriptionUpdate(String, bool), // text, is_final
    TranscriptionComplete(String),

    // Real-time feedback

    // Model management
    ModelLoading,
    ModelLoaded,
    ModelUnloaded,

    // Session management
    SessionCleared,
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
            text: initial_text.unwrap_or_else(|| String::new()),
            complete_text: String::new(),
            daemon_connected: false,
            is_recording: false,
            session_id: None,

            // Initialize UI state
            model_loading: false,

            rx,
            _tx: tx,
        }
    }

    fn process_messages(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                UiMessage::DaemonConnected(connected) => {
                    self.daemon_connected = connected;
                    // Daemon connection handled at startup - no need for status messages
                }
                UiMessage::RecordingStarted(session_id) => {
                    self.session_id = Some(session_id);
                    self.is_recording = true;
                    self.text = "".to_string();
                    self.complete_text = String::new();
                }
                UiMessage::RecordingStopped => {
                    self.is_recording = false;
                    // Keep the final text for copying
                }
                UiMessage::TranscriptionUpdate(new_text, is_final) => {
                    // Accumulate text chunks from daemon
                    if !self.text.is_empty() && !new_text.trim().is_empty() {
                        self.text.push(' ');
                    }
                    self.text.push_str(&new_text);
                    log::info!("Update: '{}' (final: {})", new_text, is_final);
                }
                UiMessage::TranscriptionComplete(final_text) => {
                    // Add to complete text and clear current partial
                    if !self.complete_text.is_empty() && !final_text.trim().is_empty() {
                        self.complete_text.push(' ');
                    }
                    self.complete_text.push_str(&final_text);
                    self.text = self.complete_text.clone();
                    self.is_recording = false;
                    log::info!("Complete: '{}'", final_text);
                }
                // Real-time feedback messages (audio level removed)
                // Model management messages
                UiMessage::ModelLoading => {
                    self.model_loading = true;
                }
                UiMessage::ModelLoaded => {
                    self.model_loading = false;
                }
                UiMessage::ModelUnloaded => {
                    self.model_loading = false;
                }
                UiMessage::SessionCleared => {
                    self.complete_text.clear();
                    self.text.clear();
                }
                UiMessage::Error(_error) => {
                    // Errors handled by daemon exit - no UI feedback needed
                }
            }
        }
    }

    fn stop_recording(&mut self) {
        if self.is_recording {
            // Immediately update UI state
            self.is_recording = false;

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

            // Spawn wl-copy and let it run in background (required for Wayland)
            match std::process::Command::new("wl-copy")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    if let Some(mut stdin) = child.stdin.take() {
                        if stdin.write_all(self.text.as_bytes()).is_ok() {
                            drop(stdin); // Close stdin
                            // Don't wait for child - let wl-copy run in background

                            // Use notify-send for clipboard notification
                            let _ = std::process::Command::new("notify-send")
                                .arg("Voice Dictation")
                                .arg("Text copied to clipboard")
                                .arg("--expire-time=2000")
                                .spawn();

                            log::info!("Successfully copied to clipboard");
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to spawn wl-copy: {}", e);
                }
            }
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

            ui.heading("Voice Dictation");

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

fn daemon_communication_thread(tx: mpsc::Sender<UiMessage>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Check daemon status
        match check_daemon_status().await {
            Ok(true) => {
                let _ = tx.send(UiMessage::DaemonConnected(true));

                // Start recording
                match send_start_recording(tx.clone()).await {
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
                // Daemon not available - show notification and exit
                let _ = std::process::Command::new("notify-send")
                    .arg("Voice Dictation")
                    .arg("Daemon not available. Please start the daemon first.")
                    .arg("--urgency=critical")
                    .spawn();

                std::process::exit(1);
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

async fn send_start_recording(tx: mpsc::Sender<UiMessage>) -> Result<(Uuid, UnixStream)> {
    let socket_path = "/tmp/dictation.sock";
    let mut stream = UnixStream::connect(socket_path).await?;

    protocol::send_message(&mut stream, &ClientMessage::StartRecording).await?;

    // The daemon might send ModelLoading -> ModelLoaded -> RecordingStarted
    // We need to wait for RecordingStarted specifically
    loop {
        match protocol::receive_message::<DaemonMessage>(&mut stream).await? {
            DaemonMessage::RecordingStarted(session_id) => {
                return Ok((session_id, stream));
            }
            DaemonMessage::ModelLoading => {
                // Forward model loading message to UI
                let _ = tx.send(UiMessage::ModelLoading);
                continue;
            }
            DaemonMessage::ModelLoaded => {
                // Forward model loaded message to UI
                let _ = tx.send(UiMessage::ModelLoaded);
                continue;
            }
            DaemonMessage::Error(error) => {
                anyhow::bail!("Daemon error: {}", error);
            }
            other => {
                anyhow::bail!("Unexpected response from daemon: {:?}", other);
            }
        }
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
                    DaemonMessage::TranscriptionUpdate { session_id: msg_session_id, partial_text, is_final } => {
                        if msg_session_id == session_id {
                            let _ = tx.send(UiMessage::TranscriptionUpdate(partial_text, is_final));
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
                    // Real-time feedback messages (audio level removed)
                    // Model management messages
                    DaemonMessage::ModelLoading => {
                        let _ = tx.send(UiMessage::ModelLoading);
                    }
                    DaemonMessage::ModelLoaded => {
                        let _ = tx.send(UiMessage::ModelLoaded);
                    }
                    DaemonMessage::ModelUnloaded => {
                        let _ = tx.send(UiMessage::ModelUnloaded);
                    }
                    DaemonMessage::SessionCleared => {
                        let _ = tx.send(UiMessage::SessionCleared);
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