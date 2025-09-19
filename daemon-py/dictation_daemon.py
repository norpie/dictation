#!/usr/bin/env python3
"""
Simple Python daemon using RealtimeSTT for streaming transcription.
RealtimeSTT handles all the voice detection and streaming logic.
"""

import os
import asyncio
import socket
import struct
import msgpack
import logging
from pathlib import Path
import uuid
import numpy as np
import pyaudio
import faster_whisper
import threading
import queue
from collections import deque

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class DictationDaemon:
    def __init__(self):
        self.current_session_id = None
        self.client_writer = None
        self.transcription_complete = False

        # Audio recording state
        self.recording = False
        self.audio_thread = None
        self.transcription_thread = None

        # Overlap handling for continuous transcription
        self.full_transcript = ""  # Complete internal transcript
        self.last_sent_length = 0  # Track what we've already sent

        # Audio parameters
        self.CHUNK = 1024
        self.FORMAT = pyaudio.paInt16
        self.CHANNELS = 1
        self.RATE = 16000

        # Audio buffer and queue - longer buffer for better context
        self.audio_buffer = deque(maxlen=int(self.RATE * 10 / self.CHUNK))  # 10 seconds
        self.audio_queue = queue.Queue()
        self.chunk_counter = 0

        # Event loop for async operations from threads
        self.loop = None

        # Initialize faster-whisper model directly
        logger.info("Initializing faster-whisper model...")
        try:
            self.model = faster_whisper.WhisperModel("distil-large-v3", device="cuda")
            logger.info("✅ faster-whisper model initialized successfully")
        except Exception as e:
            logger.error(f"❌ Failed to initialize faster-whisper model: {e}")
            raise

    def normalize_for_matching(self, text):
        """Normalize text for overlap detection"""
        import re
        # Remove punctuation and convert to lowercase
        text = re.sub(r'[^\w\s]', '', text.lower())
        # Normalize whitespace
        return ' '.join(text.split())

    def find_longest_common_overlap(self, existing_text, new_text):
        """Find the longest overlap between end of existing_text and start of new_text"""
        if not existing_text or not new_text:
            return 0

        # Normalize both texts for comparison
        existing_norm = self.normalize_for_matching(existing_text)
        new_norm = self.normalize_for_matching(new_text)

        logger.debug(f"Normalized existing: '{existing_norm}'")
        logger.debug(f"Normalized new: '{new_norm}'")

        # Simple approach: look for the longest suffix of existing that matches prefix of new
        existing_words = existing_norm.split()
        new_words = new_norm.split()

        max_overlap = min(len(existing_words), len(new_words))

        # Find longest word-based overlap
        for overlap_len in range(max_overlap, 0, -1):
            if existing_words[-overlap_len:] == new_words[:overlap_len]:
                logger.debug(f"Found {overlap_len}-word overlap: '{' '.join(new_words[:overlap_len])}'")

                # Now find where this overlap ends in the original new_text
                # Simple approach: split original text and count words
                original_words = new_text.split()
                char_pos = 0
                words_counted = 0

                for word in original_words:
                    if words_counted < overlap_len:
                        char_pos += len(word) + 1  # +1 for space
                        words_counted += 1
                    else:
                        break

                return char_pos

        return 0

    def update_transcript(self, new_transcription):
        """Update internal transcript and return only new content for partial updates"""
        if not new_transcription.strip():
            return ""

        # For the first transcription, just use it
        if not self.full_transcript:
            self.full_transcript = new_transcription.strip()
            logger.info(f"First transcription: '{new_transcription.strip()}'")
            return new_transcription.strip()

        logger.info(f"Looking for overlap between:")
        logger.info(f"  Existing: '{self.full_transcript}'")
        logger.info(f"  New:      '{new_transcription}'")

        # Find overlap position
        overlap_pos = self.find_longest_common_overlap(self.full_transcript, new_transcription)
        logger.info(f"Overlap position found: {overlap_pos}")

        if overlap_pos > 0:
            # Extract only the new part
            new_part = new_transcription[overlap_pos:].strip()
            if new_part:
                self.full_transcript = self.full_transcript + " " + new_part
                logger.info(f"OVERLAP DETECTED - Added new content: '{new_part}'")
                return new_part
            else:
                logger.info("OVERLAP DETECTED - No new content")
                return ""
        else:
            # Check if new transcription is completely contained in existing
            if new_transcription.lower().strip() in self.full_transcript.lower():
                logger.info(f"CONTAINED - New transcription already exists in full transcript")
                return ""

            # No overlap found - this might be a completely new sentence
            self.full_transcript = self.full_transcript + " " + new_transcription
            logger.info(f"NO OVERLAP - Appending entire new transcription: '{new_transcription}'")
            return new_transcription

    async def start_server(self):
        """Start the Unix domain socket server"""
        # Store the event loop for thread communication
        self.loop = asyncio.get_event_loop()

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
            if self.recording:
                self.recording = False
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
        """Start recording with faster-whisper"""
        try:
            # Generate session ID
            session_uuid = uuid.uuid4()
            self.current_session_id = str(session_uuid)
            self.transcription_complete = False

            # Reset transcript tracking for new session
            self.full_transcript = ""
            self.last_sent_length = 0

            # Send recording started message (UUID as bytes)
            await self.send_message({'RecordingStarted': session_uuid.bytes})

            # Start recording
            self.recording = True

            # Start audio capture thread
            self.audio_thread = threading.Thread(target=self._audio_capture)
            self.audio_thread.start()

            # Start transcription thread
            self.transcription_thread = threading.Thread(target=self._transcription_worker)
            self.transcription_thread.start()

            logger.info("🎤 Live transcription started")

        except Exception as e:
            logger.error(f"Error starting recording: {e}")
            await self.send_message({'Error': str(e)})

    def _audio_capture(self):
        """Capture audio in real-time"""
        p = pyaudio.PyAudio()
        stream = p.open(format=self.FORMAT,
                       channels=self.CHANNELS,
                       rate=self.RATE,
                       input=True,
                       frames_per_buffer=self.CHUNK)

        try:
            while self.recording:
                data = stream.read(self.CHUNK, exception_on_overflow=False)
                self.audio_buffer.append(data)
                self.chunk_counter += 1

                # Every 3 seconds, queue audio for transcription (longer segments)
                if self.chunk_counter % int(self.RATE * 3 / self.CHUNK) == 0:
                    # Convert buffer to numpy array
                    audio_data = b''.join(list(self.audio_buffer))
                    audio_np = np.frombuffer(audio_data, dtype=np.int16).astype(np.float32) / 32768.0

                    # Check audio level and queue for transcription
                    max_amplitude = np.max(np.abs(audio_np))
                    logger.info(f"Audio chunk: max_amplitude={max_amplitude:.4f}, length={len(audio_np)/self.RATE:.2f}s")

                    # Lower threshold for better sensitivity
                    if max_amplitude > 0.005:
                        logger.info(f"Queueing audio chunk for transcription")
                        self.audio_queue.put(audio_np.copy())
                    else:
                        logger.info(f"Skipping quiet audio chunk")

        except Exception as e:
            logger.error(f"Audio capture error: {e}")
        finally:
            stream.stop_stream()
            stream.close()
            p.terminate()

    def _transcription_worker(self):
        """Process audio chunks for transcription"""
        logger.info("Transcription worker started")
        while self.recording or not self.audio_queue.empty():
            try:
                # Get audio chunk with timeout
                audio_chunk = self.audio_queue.get(timeout=0.5)
                logger.info(f"Got audio chunk for transcription, length: {len(audio_chunk)/self.RATE:.2f}s")

                # Transcribe with settings optimized for continuous speech
                logger.info("Starting transcription...")
                segments, info = self.model.transcribe(
                    audio_chunk,
                    language="en",
                    vad_filter=True,
                    vad_parameters=dict(min_silence_duration_ms=200),
                    beam_size=5,
                    best_of=5
                )
                logger.info(f"Transcription complete, detected language: {info.language}, segments: {len(list(segments))}")

                # Process segments again since iterator was consumed
                segments, info = self.model.transcribe(
                    audio_chunk,
                    language="en",
                    vad_filter=True,
                    vad_parameters=dict(min_silence_duration_ms=200),
                    beam_size=5,
                    best_of=5
                )

                # Combine all segments into one transcription
                current_transcription = " ".join(segment.text.strip() for segment in segments if segment.text.strip())

                if current_transcription:
                    logger.info(f"Current transcription: '{current_transcription}'")

                    # Update internal transcript and get only new content
                    new_content = self.update_transcript(current_transcription)

                    # Send only new content as partial update
                    if new_content and self.client_writer and not self.transcription_complete:
                        session_uuid_bytes = uuid.UUID(self.current_session_id).bytes

                        # Send only the new part as partial update
                        if self.loop and not self.loop.is_closed():
                            logger.info(f"Sending partial update: '{new_content}'")
                            asyncio.run_coroutine_threadsafe(
                                self.send_message({
                                    'TranscriptionUpdate': {
                                        'session_id': session_uuid_bytes,
                                        'partial_text': new_content
                                    }
                                }),
                                self.loop
                            )
                else:
                    logger.info("No segments detected in audio chunk")

            except queue.Empty:
                continue
            except Exception as e:
                logger.error(f"Transcription error: {e}")

    async def stop_recording(self):
        """Stop recording"""
        self.recording = False

        # Wait for threads to finish
        if self.audio_thread and self.audio_thread.is_alive():
            self.audio_thread.join(timeout=2)
        if self.transcription_thread and self.transcription_thread.is_alive():
            self.transcription_thread.join(timeout=2)

        logger.info("⏹️ Recording stopped")

        # Send final complete transcript
        if self.full_transcript and self.client_writer:
            import time
            current_time = time.time()

            # Create TranscriptionSession object matching Rust struct
            session_obj = {
                'id': uuid.UUID(self.current_session_id).bytes,
                'status': 'Completed',  # SessionStatus::Completed
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