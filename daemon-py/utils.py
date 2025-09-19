"""
Utility functions for text processing and overlap detection.
"""

import re
import logging

logger = logging.getLogger(__name__)

def normalize_for_matching(text):
    """Normalize text for overlap detection"""
    # Remove punctuation and convert to lowercase
    text = re.sub(r'[^\w\s]', '', text.lower())
    # Normalize whitespace
    return ' '.join(text.split())

def find_longest_common_overlap(existing_text, new_text):
    """Find the longest overlap between end of existing_text and start of new_text"""
    if not existing_text or not new_text:
        return 0

    # Normalize both texts for comparison
    existing_norm = normalize_for_matching(existing_text)
    new_norm = normalize_for_matching(new_text)

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