#!/usr/bin/env python3
"""
Simple Python daemon using RealtimeSTT for streaming transcription.
RealtimeSTT handles all the voice detection and streaming logic.
"""

import os

# Set ROCm environment variables BEFORE any imports
os.environ['HSA_OVERRIDE_GFX_VERSION'] = '11.0.0'
os.environ['HIP_VISIBLE_DEVICES'] = '0'
os.environ['CUDA_VISIBLE_DEVICES'] = '0'
# Additional ROCm environment variables for PyTorch
os.environ['PYTORCH_ROCM_ARCH'] = 'gfx1100'
os.environ['HSA_ENABLE_SDMA'] = '0'
# Set library path for ROCm CTranslate2
home_dir = os.path.expanduser('~')
ctranslate2_lib_path = f"{home_dir}/repos/dictation/CTranslate2-rocm/CTranslate2-rocm/build"
lib_path = f"{ctranslate2_lib_path}:{home_dir}/.local/lib64:{home_dir}/.local/lib"
if 'LD_LIBRARY_PATH' in os.environ:
    os.environ['LD_LIBRARY_PATH'] = f"{lib_path}:{os.environ['LD_LIBRARY_PATH']}"
else:
    os.environ['LD_LIBRARY_PATH'] = lib_path

import asyncio
import socket
import struct
import msgpack
import logging
from pathlib import Path
from RealtimeSTT import AudioToTextRecorder
import uuid

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class DictationDaemon:
    def __init__(self):
        self.recorder = None
        self.current_session_id = None
        self.client_writer = None
        self.transcription_complete = False

        # Pre-initialize model on startup for easier testing
        logger.info("Initializing Whisper model on startup...")
        try:
            from RealtimeSTT import AudioToTextRecorder
            # Test model initialization
            test_recorder = AudioToTextRecorder(
                model="large-v3",  # Use large-v3 model for best accuracy
                device="cuda",
                enable_realtime_transcription=False
            )
            test_recorder = None  # Clean up
            logger.info("✅ Whisper model initialization successful")
        except Exception as e:
            logger.error(f"❌ Failed to initialize Whisper model: {e}")
            raise

    async def start_server(self):
        """Start the Unix domain socket server"""
        socket_path = Path("/tmp/dictation.sock")

        # Remove existing socket file
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
                # Read message length
                length_data = await reader.read(4)
                if not length_data:
                    break

                length = struct.unpack('<I', length_data)[0]

                # Read message data
                data = await reader.read(length)
                if not data:
                    break

                message = msgpack.unpackb(data, raw=False)
                await self.handle_message(message)

        except Exception as e:
            logger.error(f"Error handling client: {e}")
        finally:
            writer.close()
            await writer.wait_closed()
            self.client_writer = None
            if self.recorder:
                self.recorder.stop()
            logger.info("Client disconnected")

    async def handle_message(self, message):
        """Handle incoming message from client"""
        # Message is a dict with enum variant as key
        if 'StartRecording' in message:
            await self.start_recording()
        elif 'StopRecording' in message:
            await self.stop_recording()
        elif 'GetStatus' in message:
            await self.send_status()
        elif 'Shutdown' in message:
            logger.info("Shutdown requested")
            if self.recorder:
                self.recorder.stop()
            return

    async def start_recording(self):
        """Start recording with RealtimeSTT"""
        try:
            # Generate session ID
            session_uuid = uuid.uuid4()
            self.current_session_id = str(session_uuid)
            self.transcription_complete = False

            # Send recording started message (UUID as bytes)
            await self.send_message({'RecordingStarted': session_uuid.bytes})

            # Initialize RealtimeSTT with callbacks
            self.recorder = AudioToTextRecorder(
                enable_realtime_transcription=True,
                on_realtime_transcription_update=self.on_transcription_update,
                on_realtime_transcription_stabilized=self.on_transcription_finished,
                model="large-v3",  # Use large-v3 model for best accuracy
                language="en",     # English only
                device="cuda"      # ROCm appears as CUDA to PyTorch
            )

            # Start recording in background
            asyncio.create_task(self.run_recorder())

        except Exception as e:
            logger.error(f"Error starting recording: {e}")
            await self.send_message({'Error': str(e)})

    async def run_recorder(self):
        """Run the recorder in a background task"""
        try:
            # This will block until recording is stopped
            final_text = self.recorder.text()

            # Send final transcription
            if not self.transcription_complete:
                import time
                session_uuid_bytes = uuid.UUID(self.current_session_id).bytes
                await self.send_message({
                    'TranscriptionComplete': {
                        'id': session_uuid_bytes,
                        'status': 'Completed',
                        'text': final_text,
                        'confidence': 1.0,
                        'created_at': {'secs_since_epoch': int(time.time()), 'nanos_since_epoch': 0}
                    }
                })
                self.transcription_complete = True

        except Exception as e:
            logger.error(f"Error in recorder: {e}")
            if self.client_writer:
                await self.send_message({'Error': str(e)})

    def on_transcription_update(self, text):
        """Callback for real-time transcription updates"""
        if self.client_writer and not self.transcription_complete:
            # Send update in async context
            session_uuid_bytes = uuid.UUID(self.current_session_id).bytes
            asyncio.create_task(self.send_message({
                'TranscriptionUpdate': {
                    'session_id': session_uuid_bytes,
                    'partial_text': text
                }
            }))

    def on_transcription_finished(self, text):
        """Callback for final transcription"""
        if self.client_writer and not self.transcription_complete:
            self.transcription_complete = True
            # Send final result
            import time
            session_uuid_bytes = uuid.UUID(self.current_session_id).bytes
            asyncio.create_task(self.send_message({
                'TranscriptionComplete': {
                    'id': session_uuid_bytes,
                    'status': 'Completed',
                    'text': text,
                    'confidence': 1.0,
                    'created_at': {'secs_since_epoch': int(time.time()), 'nanos_since_epoch': 0}
                }
            }))

    async def stop_recording(self):
        """Stop recording"""
        if self.recorder:
            self.recorder.stop()
            self.recorder = None

        await self.send_message('RecordingStopped')

    async def send_status(self):
        """Send daemon status"""
        # Convert session ID back to UUID bytes for status
        active_sessions = []
        if self.current_session_id:
            active_sessions = [uuid.UUID(self.current_session_id).bytes]

        await self.send_message({
            'Status': {
                'model_loaded': True,  # RealtimeSTT handles model loading
                'active_sessions': active_sessions,
                'uptime': {'secs': 0, 'nanos': 0}  # TODO: track uptime
            }
        })

    async def send_message(self, message):
        """Send message to client"""
        if not self.client_writer:
            return

        try:
            data = msgpack.packb(message)
            length = struct.pack('<I', len(data))

            self.client_writer.write(length + data)
            await self.client_writer.drain()

        except Exception as e:
            logger.error(f"Error sending message: {e}")

async def main():
    daemon = DictationDaemon()
    await daemon.start_server()

if __name__ == "__main__":
    asyncio.run(main())