"""
Transcription processing for the dictation daemon.
"""

import asyncio
import logging
import threading
import queue
from utils import find_longest_common_overlap

logger = logging.getLogger(__name__)

class TranscriptionHandler:
    def __init__(self, model_manager):
        self.model_manager = model_manager
        self.processing = False
        self.transcription_thread = None
        self.loop = None

        # Transcript tracking
        self.full_transcript = ""

    def set_loop(self, loop):
        """Set the asyncio event loop for cross-thread communication"""
        self.loop = loop

    async def start_processing(self, audio_queue, callback):
        """Start transcription processing"""
        if self.processing:
            return

        self.processing = True
        self.full_transcript = ""
        self.callback = callback

        # Start transcription thread
        self.transcription_thread = threading.Thread(
            target=self._transcription_worker,
            args=(audio_queue,)
        )
        self.transcription_thread.start()

        logger.info("🔄 Transcription processing started")

    async def stop_processing(self):
        """Stop transcription processing"""
        if not self.processing:
            return

        self.processing = False

        # Wait for transcription thread to finish
        if self.transcription_thread and self.transcription_thread.is_alive():
            self.transcription_thread.join(timeout=2.0)
            if self.transcription_thread.is_alive():
                logger.warning("⚠️ Transcription thread did not stop gracefully")

        logger.info("🏁 Transcription processing stopped")

    def _transcription_worker(self, audio_queue):
        """Process audio chunks for transcription (runs in separate thread)"""
        logger.info("Transcription worker started")

        while self.processing or not audio_queue.empty():
            try:
                # Get audio chunk with timeout
                audio_chunk = audio_queue.get(timeout=0.5)
                logger.info(f"Got audio chunk for transcription, length: {len(audio_chunk)/16000:.2f}s")

                # Update model activity
                self.model_manager.update_activity()

                # Send processing started feedback
                if self.loop and not self.loop.is_closed():
                    asyncio.run_coroutine_threadsafe(
                        self._send_processing_started(), self.loop
                    )

                # Get model and transcribe
                model = self.model_manager.get_model()

                logger.info("Starting transcription...")
                segments, info = model.transcribe(
                    audio_chunk,
                    language="en",
                    vad_filter=True,
                    vad_parameters=dict(min_silence_duration_ms=200),
                    beam_size=5,
                    best_of=5,
                    word_timestamps=True
                )

                # Process segments
                segment_list = list(segments)
                current_transcription = " ".join(
                    segment.text.strip() for segment in segment_list
                    if segment.text.strip()
                )

                if current_transcription:
                    logger.info(f"Current transcription: '{current_transcription}' ({len(segment_list)} segments)")

                    # Find new content using overlap detection
                    new_content = self._update_transcript(current_transcription)

                    # Send new content via callback
                    if new_content and self.loop and not self.loop.is_closed():
                        asyncio.run_coroutine_threadsafe(
                            self.callback(new_content), self.loop
                        )

                    # Send processing complete feedback
                    if self.loop and not self.loop.is_closed():
                        asyncio.run_coroutine_threadsafe(
                            self._send_processing_complete(), self.loop
                        )
                else:
                    logger.info("No segments detected in audio chunk")

            except queue.Empty:
                continue
            except Exception as e:
                logger.error(f"Transcription error: {e}")

        logger.info("🏁 Transcription worker stopped")

    def _update_transcript(self, new_transcription):
        """Update internal transcript and return only new content"""
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
        overlap_pos = find_longest_common_overlap(self.full_transcript, new_transcription)
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
                logger.info("CONTAINED - New transcription already exists in full transcript")
                return ""

            # No overlap found - this might be a completely new sentence
            self.full_transcript = self.full_transcript + " " + new_transcription
            logger.info(f"NO OVERLAP - Appending entire new transcription: '{new_transcription}'")
            return new_transcription

    async def _send_processing_started(self):
        """Send processing started message"""
        logger.info("⚙️ Processing started")

    async def _send_processing_complete(self):
        """Send processing complete message"""
        logger.info("✅ Processing complete")