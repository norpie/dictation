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
import time
from collections import deque

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler(),  # Console output
        logging.FileHandler('dictation_daemon.log')  # File output
    ]
)
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

        # Real-time feedback state
        self.vad_sensitivity = 0.5  # Voice activity detection sensitivity (0.0-1.0)
        self.current_audio_level = 0.0
        self.voice_activity_detected = False

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

        # Model management
        self.model = None
        self.model_loaded = False
        self.last_activity_time = time.time()
        self.model_timeout_seconds = 30  # 30 seconds for testing
        self.timeout_task = None

        # Don't load model at startup - load on demand to save VRAM
        logger.info("Daemon ready - model will be loaded when needed")

    def load_model(self):
        """Load the faster-whisper model into VRAM"""
        if self.model_loaded:
            return

        logger.info("🔄 Loading faster-whisper model...")
        try:
            self.model = faster_whisper.WhisperModel("distil-large-v3", device="cuda")
            self.model_loaded = True
            logger.info("✅ faster-whisper model loaded successfully")

            # Send model loaded message to clients if connected
            if self.client_writer:
                asyncio.run_coroutine_threadsafe(
                    self.send_message({'ModelLoaded': None}),
                    self.loop
                )
        except Exception as e:
            logger.error(f"❌ Failed to load faster-whisper model: {e}")
            raise

    def unload_model(self):
        """Unload the model to free VRAM"""
        if not self.model_loaded:
            return

        logger.info("🗑️ Unloading faster-whisper model to free VRAM...")
        self.model = None
        self.model_loaded = False
        logger.info("✅ Model unloaded successfully")

        # Send model unloaded message to clients if connected
        if self.client_writer:
            asyncio.run_coroutine_threadsafe(
                self.send_message({'ModelUnloaded': None}),
                self.loop
            )

    def update_activity_time(self):
        """Update the last activity time and reset timeout"""
        self.last_activity_time = time.time()

        # Cancel existing timeout task
        if self.timeout_task and not self.timeout_task.done():
            self.timeout_task.cancel()

        # Schedule new timeout task (only if we have a loop and we're in the main thread)
        if self.loop and not self.loop.is_closed():
            try:
                self.timeout_task = asyncio.run_coroutine_threadsafe(
                    self._model_timeout_task(), self.loop
                ).result() if hasattr(asyncio, 'current_task') else None

                # Better approach: schedule from the loop thread
                if hasattr(self.loop, 'call_soon_threadsafe'):
                    def schedule_timeout():
                        self.timeout_task = asyncio.create_task(self._model_timeout_task())
                    self.loop.call_soon_threadsafe(schedule_timeout)
            except Exception as e:
                logger.debug(f"Could not schedule timeout task: {e}")

    async def _model_timeout_task(self):
        """Task that unloads the model after timeout"""
        try:
            await asyncio.sleep(self.model_timeout_seconds)
            if self.model_loaded:
                logger.info(f"⏰ Model timeout reached ({self.model_timeout_seconds}s), unloading model")
                self.unload_model()
        except asyncio.CancelledError:
            # Timeout was reset, ignore
            pass

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
            # Comprehensive cleanup on client disconnect
            logger.info("🔌 Client disconnecting, cleaning up...")

            # Stop recording if active
            if self.recording:
                logger.info("🛑 Stopping recording due to disconnect")
                self.recording = False
                self.transcription_complete = True

            # Clean up audio threads
            if hasattr(self, 'audio_thread') and self.audio_thread and self.audio_thread.is_alive():
                logger.info("🧹 Stopping audio thread")
                # Audio thread will stop when self.recording becomes False
                self.audio_thread.join(timeout=2.0)
                if self.audio_thread.is_alive():
                    logger.warning("⚠️ Audio thread did not stop gracefully")

            if hasattr(self, 'transcription_thread') and self.transcription_thread and self.transcription_thread.is_alive():
                logger.info("🧹 Stopping transcription thread")
                # Transcription thread will stop when self.recording becomes False
                self.transcription_thread.join(timeout=2.0)
                if self.transcription_thread.is_alive():
                    logger.warning("⚠️ Transcription thread did not stop gracefully")

            # Clear audio queue
            if hasattr(self, 'audio_queue'):
                while not self.audio_queue.empty():
                    try:
                        self.audio_queue.get_nowait()
                    except:
                        break

            # Reset session state
            logger.info(f"🧹 Resetting session state on disconnect (transcript was: '{self.full_transcript[:50]}...')")
            self.current_session_id = None
            self.full_transcript = ""
            self.last_sent_length = 0
            self.voice_activity_detected = False
            self.current_audio_level = 0.0

            # Close connection safely
            try:
                writer.close()
                await writer.wait_closed()
            except (BrokenPipeError, ConnectionResetError, asyncio.CancelledError) as e:
                logger.debug(f"Expected error during connection close: {e}")
            except Exception as e:
                logger.warning(f"Unexpected error during connection close: {e}")
            finally:
                self.client_writer = None

            logger.info("✅ Client disconnected, cleanup complete")

    async def handle_message(self, message):
        """Handle incoming message from client"""
        # Message is a dict with enum variant as key
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
            if self.recorder:
                self.recorder.stop()
            return

    async def start_recording(self):
        """Start recording with faster-whisper"""
        try:
            # Update activity time and reset timeout
            self.update_activity_time()

            # Check if model needs to be loaded
            if not self.model_loaded:
                logger.info("🔄 Model not loaded, loading before starting recording...")
                await self.send_message({'ModelLoading': None})

                # Load model in a separate thread to avoid blocking
                def load_model_sync():
                    self.load_model()

                await asyncio.get_event_loop().run_in_executor(None, load_model_sync)

            # Generate session ID
            session_uuid = uuid.uuid4()
            self.current_session_id = str(session_uuid)
            self.transcription_complete = False

            # Reset transcript tracking for new session
            logger.info(f"🔄 Resetting transcript for new session (was: '{self.full_transcript[:50]}...')")
            self.full_transcript = ""
            self.last_sent_length = 0

            # Clear any remaining audio from previous session
            while not self.audio_queue.empty():
                try:
                    self.audio_queue.get_nowait()
                    logger.info("🧹 Cleared old audio chunk from queue")
                except:
                    break

            # Clear the audio buffer to prevent old audio carryover
            self.audio_buffer.clear()
            logger.info("🧹 Cleared audio buffer from previous session")

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
            while self.recording and self.client_writer:
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
            logger.info("🏁 Audio capture stopped")

    def _transcription_worker(self):
        """Process audio chunks for transcription"""
        logger.info("Transcription worker started")
        while (self.recording or not self.audio_queue.empty()) and self.client_writer:
            try:
                # Get audio chunk with timeout
                audio_chunk = self.audio_queue.get(timeout=0.5)
                logger.info(f"Got audio chunk for transcription, length: {len(audio_chunk)/self.RATE:.2f}s")

                # Update activity time on each transcription
                self.update_activity_time()

                # Send processing started feedback
                if self.loop and not self.loop.is_closed():
                    asyncio.run_coroutine_threadsafe(self.send_processing_started(), self.loop)

                # Transcribe with settings optimized for continuous speech
                logger.info("Starting transcription...")
                segments, info = self.model.transcribe(
                    audio_chunk,
                    language="en",
                    vad_filter=True,
                    vad_parameters=dict(min_silence_duration_ms=200),
                    beam_size=5,
                    best_of=5,
                    word_timestamps=True  # Enable word-level timestamps
                )
                logger.info(f"Transcription complete, detected language: {info.language}, segments: {len(list(segments))}")

                # Process segments again since iterator was consumed
                segments, info = self.model.transcribe(
                    audio_chunk,
                    language="en",
                    vad_filter=True,
                    vad_parameters=dict(min_silence_duration_ms=200),
                    beam_size=5,
                    best_of=5,
                    word_timestamps=True  # Enable word-level timestamps
                )

                # Process segments and collect detailed information
                segment_list = list(segments)
                current_transcription = " ".join(segment.text.strip() for segment in segment_list if segment.text.strip())

                if current_transcription:
                    logger.info(f"Current transcription: '{current_transcription}' ({len(segment_list)} segments)")

                    # Use advanced overlap detection with word-level timestamps
                    new_content = self.update_transcript_advanced(segment_list)

                    # Send only new content as partial update (for backward compatibility)
                    if new_content and self.client_writer and not self.transcription_complete:
                        session_uuid_bytes = uuid.UUID(self.current_session_id).bytes

                        # Send only the new part as partial update
                        if self.loop and not self.loop.is_closed():
                            logger.info(f"Sending partial update: '{new_content}'")
                            asyncio.run_coroutine_threadsafe(
                                self.send_message({
                                    'TranscriptionUpdate': {
                                        'session_id': session_uuid_bytes,
                                        'partial_text': new_content,
                                        'is_final': False  # TODO: determine if this is final segment
                                    }
                                }),
                                self.loop
                            )

                            # Send processing complete feedback
                            asyncio.run_coroutine_threadsafe(self.send_processing_complete(), self.loop)
                else:
                    logger.info("No segments detected in audio chunk")

            except queue.Empty:
                continue
            except Exception as e:
                logger.error(f"Transcription error: {e}")

        logger.info("🏁 Transcription worker stopped")

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
                'model_loaded': self.model_loaded,
                'active_sessions': active_sessions,
                'uptime': {'secs': 0, 'nanos': 0},  # TODO: track uptime
                'audio_device': "default",  # TODO: get actual device name
                'buffer_size': self.CHUNK,
                'vad_sensitivity': self.vad_sensitivity
            }
        })

    async def clear_session(self):
        """Clear the current session and transcript"""
        logger.info("🧹 Clearing session")
        self.full_transcript = ""
        self.last_sent_length = 0

        # Send confirmation
        await self.send_message({'SessionCleared': None})

    async def set_sensitivity(self, sensitivity):
        """Set voice activity detection sensitivity"""
        self.vad_sensitivity = max(0.0, min(1.0, sensitivity))  # Clamp to 0.0-1.0
        logger.info(f"🎚️ VAD sensitivity set to {self.vad_sensitivity}")

    async def send_audio_level(self, level):
        """Send current audio level to client"""
        self.current_audio_level = level
        if self.client_writer and not self.transcription_complete:
            await self.send_message({'AudioLevel': level})

    async def send_voice_activity_detected(self):
        """Send voice activity detected message"""
        if not self.voice_activity_detected:
            self.voice_activity_detected = True
            logger.info("🎤 Voice activity detected")
            if self.client_writer and not self.transcription_complete:
                await self.send_message({'VoiceActivityDetected': None})

    async def send_voice_activity_ended(self):
        """Send voice activity ended message"""
        if self.voice_activity_detected:
            self.voice_activity_detected = False
            logger.info("🔇 Voice activity ended")
            if self.client_writer and not self.transcription_complete:
                await self.send_message({'VoiceActivityEnded': None})

    async def send_processing_started(self):
        """Send processing started message"""
        logger.info("⚙️ Processing started")
        if self.client_writer and not self.transcription_complete:
            await self.send_message({'ProcessingStarted': None})

    async def send_processing_complete(self):
        """Send processing complete message"""
        logger.info("✅ Processing complete")
        if self.client_writer and not self.transcription_complete:
            await self.send_message({'ProcessingComplete': None})

    def update_transcript_advanced(self, segments):
        """Advanced transcript update using word-level timestamps and probabilities"""
        if not segments:
            return ""

        # Extract all words with timestamps from current segments
        current_words = []
        for segment in segments:
            if hasattr(segment, 'words') and segment.words:
                for word in segment.words:
                    current_words.append({
                        'word': word.word.strip(),
                        'start': word.start,
                        'end': word.end,
                        'probability': word.probability,
                        'segment_text': segment.text.strip()
                    })
            else:
                # Fallback if no word-level data
                words_in_segment = segment.text.strip().split()
                for i, word in enumerate(words_in_segment):
                    current_words.append({
                        'word': word,
                        'start': segment.start + (i * (segment.end - segment.start) / len(words_in_segment)),
                        'end': segment.start + ((i + 1) * (segment.end - segment.start) / len(words_in_segment)),
                        'probability': 1.0,  # Unknown, assume high
                        'segment_text': segment.text.strip()
                    })

        if not current_words:
            return ""

        # If this is the first transcription, use it all
        if not self.full_transcript:
            new_text = " ".join(word['word'] for word in current_words)
            self.full_transcript = new_text
            logger.info(f"🆕 First transcription: '{new_text}'")
            return new_text

        # Find overlap with existing transcript using word matching
        existing_words = self.full_transcript.split()

        # Look for best overlap by comparing word sequences
        best_overlap = 0
        best_new_start = 0

        # Try different starting points in current words
        for start_idx in range(len(current_words)):
            current_sequence = [w['word'] for w in current_words[start_idx:]]

            # Find longest match with end of existing transcript
            max_overlap_len = min(len(existing_words), len(current_sequence))

            for overlap_len in range(max_overlap_len, 0, -1):
                existing_suffix = existing_words[-overlap_len:]
                current_prefix = current_sequence[:overlap_len]

                # Fuzzy match allowing for minor differences
                if self.sequences_match(existing_suffix, current_prefix):
                    if overlap_len > best_overlap:
                        best_overlap = overlap_len
                        best_new_start = start_idx + overlap_len
                    break

        # Extract only truly new content
        if best_new_start < len(current_words):
            new_words = current_words[best_new_start:]
            new_content = " ".join(word['word'] for word in new_words)

            if new_content.strip():
                self.full_transcript += " " + new_content
                logger.info(f"🔄 Added new content: '{new_content}' (overlap: {best_overlap} words)")
                return new_content

        logger.info(f"🔄 No new content found (overlap: {best_overlap} words)")
        return ""

    def sequences_match(self, seq1, seq2, threshold=0.8):
        """Check if two word sequences match with fuzzy matching"""
        if len(seq1) != len(seq2):
            return False

        matches = 0
        for w1, w2 in zip(seq1, seq2):
            # Exact match or very similar (handle punctuation differences)
            w1_clean = w1.lower().strip('.,!?;:"\'')
            w2_clean = w2.lower().strip('.,!?;:"\'')

            if w1_clean == w2_clean or abs(len(w1_clean) - len(w2_clean)) <= 1:
                matches += 1

        return (matches / len(seq1)) >= threshold

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

async def main():
    daemon = DictationDaemon()
    await daemon.start_server()

if __name__ == "__main__":
    asyncio.run(main())