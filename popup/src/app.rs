use std::sync::mpsc;
use uuid::Uuid;

use crate::config::{Config, load_config};
use crate::daemon_comm::{start_daemon_communication_thread, send_stop_recording};

pub struct DictationApp {
    pub text: String,
    pub complete_text: String,
    pub daemon_connected: bool,
    pub is_recording: bool,
    pub session_id: Option<Uuid>,

    // UI state
    pub model_loading: bool,
    pub show_settings: bool,

    // Config
    pub config: Config,

    pub rx: mpsc::Receiver<UiMessage>,
    pub _tx: mpsc::Sender<UiMessage>, // Keep sender alive
}

#[derive(Debug)]
pub enum UiMessage {
    DaemonConnected(bool),
    RecordingStarted(Uuid),
    RecordingStopped,
    TranscriptionUpdate(String, bool), // text, is_final
    TranscriptionComplete(String),

    // Model management
    ModelLoading,
    ModelLoaded,
    ModelUnloaded,

    // Session management
    SessionCleared,
    Error(String),
}

impl DictationApp {
    pub fn new(initial_text: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();

        // Load config
        let config = load_config();

        // Start daemon communication in background thread
        start_daemon_communication_thread(tx.clone());

        Self {
            text: initial_text.unwrap_or_else(|| String::new()),
            complete_text: String::new(),
            daemon_connected: false,
            is_recording: false,
            session_id: None,

            // Initialize UI state
            model_loading: false,
            show_settings: false,

            // Config
            config,

            rx,
            _tx: tx,
        }
    }

    pub fn process_messages(&mut self) {
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
                    log::info!("RecordingStopped received");
                    self.is_recording = false;
                    // Keep the final text for copying

                    // Auto-copy if enabled and we have text
                    if self.config.auto_copy() && !self.text.trim().is_empty() {
                        log::info!("Auto-copy triggered on RecordingStopped");
                        self.copy_to_clipboard();
                    } else {
                        log::info!("Auto-copy not triggered: auto_copy={}, text_empty={}",
                                  self.config.auto_copy(), self.text.trim().is_empty());
                    }
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
                    log::info!("TranscriptionComplete received: '{}'", final_text);
                    // Add to complete text and clear current partial
                    if !self.complete_text.is_empty() && !final_text.trim().is_empty() {
                        self.complete_text.push(' ');
                    }
                    self.complete_text.push_str(&final_text);
                    self.text = self.complete_text.clone();
                    self.is_recording = false;
                    log::info!("Complete: '{}'", final_text);

                    // Auto-copy if enabled
                    if self.config.auto_copy() {
                        log::info!("Auto-copy triggered on TranscriptionComplete");
                        self.copy_to_clipboard();
                    } else {
                        log::info!("Auto-copy not triggered on TranscriptionComplete: auto_copy={}", self.config.auto_copy());
                    }
                }
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

    pub fn stop_recording(&mut self) {
        if self.is_recording {
            // Immediately update UI state
            self.is_recording = false;

            // Auto-copy if enabled and we have text
            if self.config.auto_copy() && !self.text.trim().is_empty() {
                log::info!("Auto-copy triggered on stop recording button");
                self.copy_to_clipboard();
            }

            // Send stop message through a new thread to avoid blocking UI
            std::thread::spawn(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let _ = send_stop_recording().await;
                });
            });
        }
    }

    pub fn copy_to_clipboard(&mut self) {
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

                            // Auto-close after copy if enabled
                            if self.config.auto_close_after_copy() {
                                std::process::exit(0);
                            }
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