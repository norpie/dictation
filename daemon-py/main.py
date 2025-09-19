#!/usr/bin/env python3
"""
Main entry point for the dictation daemon.
"""

import asyncio
import logging
from daemon import DictationDaemon

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler(),  # Console output
        logging.FileHandler('dictation_daemon.log')  # File output
    ]
)

async def main():
    daemon = DictationDaemon()
    await daemon.start_server()

if __name__ == "__main__":
    asyncio.run(main())