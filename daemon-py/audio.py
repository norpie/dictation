"""
Audio capture and processing for the dictation daemon.
"""

import asyncio
import logging
import threading
import queue
import numpy as np
import pyaudio
from collections import deque

logger = logging.getLogger(__name__)

class AudioHandler:
    def __init__(self):
        # Audio parameters
        self.CHUNK = 1024
        self.FORMAT = pyaudio.paInt16
        self.CHANNELS = 1
        self.RATE = 16000

        # Audio state
        self.recording = False
        self.audio_thread = None

        # Audio buffer and queue
        self.audio_buffer = deque(maxlen=int(self.RATE * 10 / self.CHUNK))  # 10 seconds
        self.audio_queue = queue.Queue()
        self.chunk_counter = 0

        # Event loop for async operations
        self.loop = None
        self.send_message_callback = None

    def set_loop(self, loop):
        """Set the asyncio event loop for cross-thread communication"""
        self.loop = loop

    def set_message_callback(self, callback):
        """Set callback for sending messages to client"""
        self.send_message_callback = callback

    async def start_recording(self):
        """Start audio recording"""
        if self.recording:
            return

        # Clear old data
        self.audio_buffer.clear()
        while not self.audio_queue.empty():
            try:
                self.audio_queue.get_nowait()
            except queue.Empty:
                break

        self.recording = True
        self.chunk_counter = 0

        # Start audio capture thread
        self.audio_thread = threading.Thread(target=self._audio_capture)
        self.audio_thread.start()

        logger.info("🎤 Audio recording started")

    async def stop_recording(self):
        """Stop audio recording"""
        if not self.recording:
            return

        self.recording = False

        # Wait for audio thread to finish
        if self.audio_thread and self.audio_thread.is_alive():
            self.audio_thread.join(timeout=2.0)
            if self.audio_thread.is_alive():
                logger.warning("⚠️ Audio thread did not stop gracefully")

        logger.info("⏹️ Audio recording stopped")

    def _audio_capture(self):
        """Capture audio in real-time (runs in separate thread)"""
        p = pyaudio.PyAudio()
        stream = p.open(
            format=self.FORMAT,
            channels=self.CHANNELS,
            rate=self.RATE,
            input=True,
            frames_per_buffer=self.CHUNK
        )

        try:
            while self.recording:
                data = stream.read(self.CHUNK, exception_on_overflow=False)
                self.audio_buffer.append(data)
                self.chunk_counter += 1

                # Every 3 seconds, queue audio for transcription
                if self.chunk_counter % int(self.RATE * 3 / self.CHUNK) == 0:
                    audio_data = b''.join(list(self.audio_buffer))
                    audio_np = np.frombuffer(audio_data, dtype=np.int16).astype(np.float32) / 32768.0

                    # Check audio level
                    max_amplitude = np.max(np.abs(audio_np))
                    logger.info(f"Audio chunk: max_amplitude={max_amplitude:.4f}, length={len(audio_np)/self.RATE:.2f}s")

                    # Send audio level update
                    if self.loop:
                        asyncio.run_coroutine_threadsafe(
                            self._send_audio_level(max_amplitude), self.loop
                        )

                    # Queue if above threshold
                    if max_amplitude > 0.005:
                        logger.info("Queueing audio chunk for transcription")
                        self.audio_queue.put(audio_np.copy())
                    else:
                        logger.info("Skipping quiet audio chunk")

        except Exception as e:
            logger.error(f"Audio capture error: {e}")
        finally:
            stream.stop_stream()
            stream.close()
            p.terminate()
            logger.info("🏁 Audio capture stopped")

    async def _send_audio_level(self, level):
        """Send audio level update to client"""
        if self.send_message_callback:
            await self.send_message_callback({'AudioLevel': level})