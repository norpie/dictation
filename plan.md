# Whisper Dictation App for Wayland Linux

## Architecture Overview
**Daemon-based system** with three main components:
1. **dictation-daemon** - Background service managing Whisper model
2. **dictation-client** - User command for triggering recording
3. **dictation-popup** - UI for displaying results

## Implementation Plan

### 1. Core Technologies
- **Language**: Rust (for performance and safety)
- **Whisper Integration**: whisper.cpp with streaming support
- **Audio**: PipeWire/ALSA for microphone access
- **IPC**: Unix domain sockets + D-Bus for desktop integration
- **UI**: GTK4 for Wayland-native popups
- **Notifications**: libnotify for desktop integration

### 2. Components Structure

#### dictation-daemon
- **Model Management**: Load/unload Whisper model with configurable timeout
- **Audio Pipeline**: PipeWire integration for microphone capture
- **Streaming Transcription**: Real-time processing using whisper-stream
- **IPC Server**: Unix socket listener for client requests
- **systemd Integration**: User service with socket activation

#### dictation-client  
- **Recording Trigger**: Command-line tool bound to keybinds
- **Visual Feedback**: Recording indicator (system tray or overlay)
- **IPC Communication**: Send start/stop commands to daemon
- **Audio Streaming**: Stream audio data to daemon

#### dictation-popup
- **Results Display**: Wayland-native popup window
- **User Actions**: Copy to clipboard, edit, or discard
- **Keyboard Navigation**: Accept/reject with Enter/Escape
- **Integration**: wl-clipboard for Wayland clipboard access

### 3. Key Features
- **Smart Model Loading**: Auto-load on first use, timeout-based unloading
- **Live Streaming**: Real-time transcription with immediate feedback  
- **VAD Integration**: Voice Activity Detection for better chunking
- **Multiple Models**: Support for different Whisper model sizes
- **Configuration**: YAML config for model paths, timeouts, keybinds

### 4. Directory Structure
```
dictation/
├── src/
│   ├── daemon/          # Background service
│   ├── client/          # Recording client
│   ├── popup/           # Results UI
│   ├── shared/          # Common types and utils
│   └── config/          # Configuration handling
├── systemd/             # Service files
├── desktop/             # .desktop files
└── models/              # Whisper models
```

### 5. Installation & Setup
- **Dependencies**: whisper.cpp, PipeWire, GTK4, libnotify
- **systemd Service**: User-level daemon with socket activation
- **Desktop Integration**: Keybind setup and notifications
- **Model Download**: Automated Whisper model fetching

This design provides a responsive, efficient dictation system optimized for Wayland with proper desktop integration, real-time streaming, and user-friendly interaction patterns.