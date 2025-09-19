#!/usr/bin/env python3
"""
Main DictationDaemon class for managing recording sessions and model lifecycle.
"""

import asyncio
import socket
import struct
import msgpack
import logging
import uuid
import time
import threading
from pathlib import Path

from audio import AudioHandler
from transcription import TranscriptionHandler
from model_manager import ModelManager
from utils import normalize_for_matching, find_longest_common_overlap
from config import load_config

logger = logging.getLogger(__name__)

class DictationDaemon:
    def __init__(self):
        self.current_session_id = None
        self.client_writer = None
        self.transcription_complete = False

        # Load configuration
        self.config = load_config()

        # Model management
        self.model_manager = ModelManager(
            model_name=self.config.whisper.model,
            timeout_seconds=self.config.whisper.model_timeout_seconds
        )

        # Audio and transcription handlers
        self.audio_handler = AudioHandler()
        self.transcription_handler = TranscriptionHandler(self.model_manager, self.config.whisper.language)

        # Session state
        self.full_transcript = ""
        self.last_sent_length = 0
        self.last_activity_time = time.time()

        # Event loop for async operations from threads
        self.loop = None

        logger.info("Daemon ready - model will be loaded when needed")

    async def start_server(self):
        """Start the Unix domain socket server"""
        self.loop = asyncio.get_event_loop()

        # Share loop with handlers
        self.model_manager.set_loop(self.loop)
        self.model_manager.set_message_callback(self.send_message)
        self.audio_handler.set_loop(self.loop)
        self.audio_handler.set_message_callback(self.send_message)
        self.transcription_handler.set_loop(self.loop)

        socket_path = Path("/tmp/dictation.sock")

        if socket_path.exists():
            socket_path.unlink()

        server = await asyncio.start_unix_server(
            self.handle_client,
            path=str(socket_path)
        )

        logger.info(f"Daemon listening on {socket_path}")
        async with server:
            await server.serve_forever()

    async def handle_client(self, reader, writer):
        """Handle client connection"""
        self.client_writer = writer
        logger.info("Client connected")

        try:
            while True:
                length_data = await reader.read(4)
                if not length_data:
                    break

                length = struct.unpack('<I', length_data)[0]
                data = await reader.read(length)
                if not data:
                    break

                message = msgpack.unpackb(data, raw=False)
                await self.handle_message(message)

        except Exception as e:
            logger.error(f"Error handling client: {e}")
        finally:
            await self.cleanup_client()

    async def cleanup_client(self):
        """Comprehensive cleanup on client disconnect"""
        logger.info("🔌 Client disconnecting, cleaning up...")

        # Stop recording if active
        if self.audio_handler.recording:
            logger.info("🛑 Stopping recording due to disconnect")
            await self.stop_recording()

        # Reset session state
        logger.info(f"🧹 Resetting session state on disconnect")
        self.current_session_id = None
        self.full_transcript = ""
        self.last_sent_length = 0

        # Close connection safely
        try:
            self.client_writer.close()
            await self.client_writer.wait_closed()
        except (BrokenPipeError, ConnectionResetError, asyncio.CancelledError) as e:
            logger.debug(f"Expected error during connection close: {e}")
        except Exception as e:
            logger.warning(f"Unexpected error during connection close: {e}")
        finally:
            self.client_writer = None

        logger.info("✅ Client disconnected, cleanup complete")

    async def handle_message(self, message):
        """Handle incoming message from client"""
        if 'StartRecording' in message:
            await self.start_recording()
        elif 'StopRecording' in message:
            await self.stop_recording()
        elif 'GetStatus' in message:
            await self.send_status()
        elif 'ClearSession' in message:
            await self.clear_session()
        elif 'SetSensitivity' in message:
            sensitivity = message['SetSensitivity']
            await self.set_sensitivity(sensitivity)
        elif 'Shutdown' in message:
            logger.info("Shutdown requested")
            return

    async def start_recording(self):
        """Start recording session"""
        try:
            # Update activity time
            self.last_activity_time = time.time()

            # Ensure model is loaded
            if not self.model_manager.is_loaded():
                await self.send_message({'ModelLoading': None})
                await self.model_manager.load_model()

            # Generate session ID
            session_uuid = uuid.uuid4()
            self.current_session_id = str(session_uuid)
            self.transcription_complete = False

            # Reset transcript tracking
            logger.info(f"🔄 Resetting transcript for new session")
            self.full_transcript = ""
            self.last_sent_length = 0

            # Start recording
            await self.audio_handler.start_recording()

            # Start transcription processing
            await self.transcription_handler.start_processing(
                self.audio_handler.audio_queue,
                self.on_transcription_update
            )

            await self.send_message({'RecordingStarted': session_uuid.bytes})
            logger.info("🎤 Live transcription started")

        except Exception as e:
            logger.error(f"Error starting recording: {e}")
            await self.send_message({'Error': str(e)})

    async def stop_recording(self):
        """Stop recording session"""
        await self.audio_handler.stop_recording()
        await self.transcription_handler.stop_processing()

        logger.info("⏹️ Recording stopped")

        # Send final complete transcript
        if self.full_transcript and self.client_writer:
            current_time = time.time()
            session_obj = {
                'id': uuid.UUID(self.current_session_id).bytes,
                'status': 'Completed',
                'text': self.full_transcript,
                'confidence': None,
                'created_at': {
                    'secs_since_epoch': int(current_time),
                    'nanos_since_epoch': int((current_time % 1) * 1_000_000_000)
                }
            }

            logger.info(f"Sending final transcript: '{self.full_transcript}'")
            await self.send_message({'TranscriptionComplete': session_obj})

        await self.send_message('RecordingStopped')

    async def on_transcription_update(self, new_content):
        """Handle transcription updates from transcription handler"""
        if new_content and self.client_writer and not self.transcription_complete:
            # Update activity time
            self.last_activity_time = time.time()

            # Update full transcript
            if self.full_transcript:
                self.full_transcript += " " + new_content
            else:
                self.full_transcript = new_content

            session_uuid_bytes = uuid.UUID(self.current_session_id).bytes

            logger.info(f"Sending partial update: '{new_content}'")
            await self.send_message({
                'TranscriptionUpdate': {
                    'session_id': session_uuid_bytes,
                    'partial_text': new_content,
                    'is_final': False
                }
            })

    async def send_status(self):
        """Send daemon status"""
        active_sessions = []
        if self.current_session_id:
            active_sessions = [uuid.UUID(self.current_session_id).bytes]

        await self.send_message({
            'Status': {
                'model_loaded': self.model_manager.is_loaded(),
                'active_sessions': active_sessions,
                'uptime': {'secs': 0, 'nanos': 0},
                'audio_device': "default",
                'buffer_size': self.audio_handler.CHUNK,
                'vad_sensitivity': 0.5  # TODO: make configurable
            }
        })

    async def clear_session(self):
        """Clear the current session and transcript"""
        logger.info("🧹 Clearing session")
        self.full_transcript = ""
        self.last_sent_length = 0
        await self.send_message({'SessionCleared': None})

    async def set_sensitivity(self, sensitivity):
        """Set voice activity detection sensitivity"""
        # TODO: implement in audio handler
        logger.info(f"🎚️ VAD sensitivity set to {sensitivity}")

    async def send_message(self, message):
        """Send message to client"""
        if not self.client_writer:
            return

        try:
            data = msgpack.packb(message)
            length = struct.pack('<I', len(data))

            self.client_writer.write(length + data)
            await self.client_writer.drain()

        except (BrokenPipeError, ConnectionResetError) as e:
            logger.debug(f"Connection lost: {e}")
            return "Connection lost"
        except Exception as e:
            logger.error(f"Error sending message: {e}")
            return str(e)