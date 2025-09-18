#!/usr/bin/env python3
"""
Simple test daemon without RealtimeSTT to test IPC functionality.
"""

import asyncio
import socket
import struct
import msgpack
import logging
from pathlib import Path
import uuid

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class TestDaemon:
    def __init__(self):
        self.current_session_id = None
        self.client_writer = None

    async def start_server(self):
        """Start the Unix domain socket server"""
        socket_path = Path("/tmp/dictation.sock")
        socket_path.parent.mkdir(parents=True, exist_ok=True)

        # Remove existing socket file
        if socket_path.exists():
            socket_path.unlink()

        server = await asyncio.start_unix_server(
            self.handle_client,
            path=str(socket_path)
        )

        logger.info(f"Test daemon listening on {socket_path}")
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
                logger.info(f"Received message: {message}")
                await self.handle_message(message)

        except Exception as e:
            logger.error(f"Error handling client: {e}")
        finally:
            writer.close()
            await writer.wait_closed()
            self.client_writer = None
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
            return

    async def start_recording(self):
        """Mock start recording"""
        try:
            # Generate session ID
            session_uuid = uuid.uuid4()
            self.current_session_id = str(session_uuid)

            # Send recording started message (UUID as bytes)
            await self.send_message({'RecordingStarted': session_uuid.bytes})

            # Mock transcription updates
            await asyncio.sleep(1)
            session_uuid_bytes = uuid.UUID(self.current_session_id).bytes

            await self.send_message({
                'TranscriptionUpdate': {
                    'session_id': session_uuid_bytes,
                    'partial_text': "Hello"
                }
            })

            await asyncio.sleep(1)
            await self.send_message({
                'TranscriptionUpdate': {
                    'session_id': session_uuid_bytes,
                    'partial_text': "Hello world"
                }
            })

            await asyncio.sleep(1)
            import time
            await self.send_message({
                'TranscriptionComplete': {
                    'id': session_uuid_bytes,
                    'status': 'Completed',
                    'text': "Hello world, this is a test!",
                    'confidence': 1.0,
                    'created_at': {'secs_since_epoch': int(time.time()), 'nanos_since_epoch': 0}
                }
            })

        except Exception as e:
            logger.error(f"Error in mock recording: {e}")
            await self.send_message({'Error': str(e)})

    async def stop_recording(self):
        """Mock stop recording"""
        await self.send_message('RecordingStopped')

    async def send_status(self):
        """Send daemon status"""
        # Convert session ID back to UUID bytes for status
        active_sessions = []
        if self.current_session_id:
            active_sessions = [uuid.UUID(self.current_session_id).bytes]

        await self.send_message({
            'Status': {
                'model_loaded': True,
                'active_sessions': active_sessions,
                'uptime': {'secs': 0, 'nanos': 0}
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
            logger.info(f"Sent message: {message}")

        except Exception as e:
            logger.error(f"Error sending message: {e}")

async def main():
    daemon = TestDaemon()
    await daemon.start_server()

if __name__ == "__main__":
    asyncio.run(main())