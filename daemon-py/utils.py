"""
Utility functions for text processing and overlap detection.
"""

import re
import logging

logger = logging.getLogger(__name__)


def calculate_edit_distance(s1, s2):
    """Calculate Levenshtein distance between two strings"""
    if len(s1) < len(s2):
        return calculate_edit_distance(s2, s1)

    if len(s2) == 0:
        return len(s1)

    previous_row = list(range(len(s2) + 1))
    for i, c1 in enumerate(s1):
        current_row = [i + 1]
        for j, c2 in enumerate(s2):
            insertions = previous_row[j + 1] + 1
            deletions = current_row[j] + 1
            substitutions = previous_row[j] + (c1 != c2)
            current_row.append(min(insertions, deletions, substitutions))
        previous_row = current_row

    return previous_row[-1]


def calculate_similarity_ratio(s1, s2):
    """Calculate similarity ratio between two strings (0.0 to 1.0)"""
    if not s1 and not s2:
        return 1.0
    if not s1 or not s2:
        return 0.0

    max_len = max(len(s1), len(s2))
    edit_distance = calculate_edit_distance(s1, s2)
    return 1.0 - (edit_distance / max_len)

def normalize_for_matching(text):
    """Normalize text for overlap detection"""
    # Remove punctuation and convert to lowercase
    text = re.sub(r'[^\w\s]', '', text.lower())
    # Normalize whitespace
    return ' '.join(text.split())

def find_longest_common_overlap(existing_text, new_text, fuzzy_threshold=0.8):
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

    # First try exact word-based overlap (fast path)
    for overlap_len in range(max_overlap, 0, -1):
        if existing_words[-overlap_len:] == new_words[:overlap_len]:
            logger.debug(f"Found exact {overlap_len}-word overlap: '{' '.join(new_words[:overlap_len])}'")
            return _calculate_char_position(new_text, overlap_len)

    # Try fuzzy matching if exact match failed
    logger.debug(f"No exact match found, trying fuzzy matching with threshold {fuzzy_threshold}")

    for overlap_len in range(max_overlap, 0, -1):
        existing_suffix = ' '.join(existing_words[-overlap_len:])
        new_prefix = ' '.join(new_words[:overlap_len])

        similarity = calculate_similarity_ratio(existing_suffix, new_prefix)
        logger.debug(f"Fuzzy match attempt: '{existing_suffix}' vs '{new_prefix}' = {similarity:.3f}")

        if similarity >= fuzzy_threshold:
            logger.debug(f"Found fuzzy {overlap_len}-word overlap (similarity: {similarity:.3f}): '{new_prefix}'")
            return _calculate_char_position(new_text, overlap_len)

    return 0


def _calculate_char_position(original_text, word_count):
    """Calculate character position after given number of words"""
    original_words = original_text.split()
    char_pos = 0
    words_counted = 0

    for word in original_words:
        if words_counted < word_count:
            char_pos += len(word) + 1  # +1 for space
            words_counted += 1
        else:
            break

    return char_pos