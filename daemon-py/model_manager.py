"""
Model lifecycle management for the dictation daemon.
Handles loading, unloading, and timeout management.
"""

import asyncio
import logging
import time
import faster_whisper

logger = logging.getLogger(__name__)

class ModelManager:
    def __init__(self, timeout_seconds=300):  # 5 minutes default
        self.model = None
        self.model_loaded = False
        self.timeout_seconds = timeout_seconds
        self.timeout_task = None
        self.loop = None
        self.send_message_callback = None

    def set_loop(self, loop):
        """Set the asyncio event loop for cross-thread communication"""
        self.loop = loop

    def set_message_callback(self, callback):
        """Set callback for sending messages to client"""
        self.send_message_callback = callback

    def is_loaded(self):
        """Check if model is currently loaded"""
        return self.model_loaded

    async def load_model(self):
        """Load the faster-whisper model into VRAM"""
        if self.model_loaded:
            return

        logger.info("🔄 Loading faster-whisper model...")
        try:
            # Run in executor to avoid blocking
            await asyncio.get_event_loop().run_in_executor(
                None, self._load_model_sync
            )
            logger.info("✅ faster-whisper model loaded successfully")

            # Send model loaded message
            if self.send_message_callback and self.loop:
                asyncio.run_coroutine_threadsafe(
                    self.send_message_callback({'ModelLoaded': None}),
                    self.loop
                )

            # Reset timeout
            self._reset_timeout()

        except Exception as e:
            logger.error(f"❌ Failed to load faster-whisper model: {e}")
            raise

    def _load_model_sync(self):
        """Synchronous model loading (runs in executor)"""
        self.model = faster_whisper.WhisperModel("distil-large-v3", device="cuda")
        self.model_loaded = True

    def unload_model(self):
        """Unload the model to free VRAM"""
        if not self.model_loaded:
            return

        logger.info("🗑️ Unloading faster-whisper model to free VRAM...")
        self.model = None
        self.model_loaded = False
        logger.info("✅ Model unloaded successfully")

        # Cancel timeout task
        if self.timeout_task and not self.timeout_task.done():
            self.timeout_task.cancel()

    def update_activity(self):
        """Update activity time and reset timeout"""
        if self.model_loaded:
            self._reset_timeout()

    def _reset_timeout(self):
        """Reset the model timeout task"""
        # Cancel existing timeout
        if self.timeout_task and not self.timeout_task.done():
            self.timeout_task.cancel()

        # Schedule new timeout if we have a loop
        if self.loop and not self.loop.is_closed():
            self.timeout_task = asyncio.run_coroutine_threadsafe(
                self._timeout_task(), self.loop
            )

    async def _timeout_task(self):
        """Task that unloads the model after timeout"""
        try:
            await asyncio.sleep(self.timeout_seconds)
            if self.model_loaded:
                logger.info(f"⏰ Model timeout reached ({self.timeout_seconds}s), unloading model")
                self.unload_model()
        except asyncio.CancelledError:
            # Timeout was reset, ignore
            pass

    def get_model(self):
        """Get the loaded model instance"""
        if not self.model_loaded:
            raise RuntimeError("Model not loaded")
        return self.model