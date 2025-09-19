#!/usr/bin/env python3
"""Test script for fuzzy overlap detection"""

import sys
import os
sys.path.insert(0, os.path.dirname(__file__))

from utils import find_longest_common_overlap, calculate_similarity_ratio

def test_exact_match():
    print("=== Testing Exact Match ===")
    existing = "hello world test"
    new_text = "test this is new"

    result = find_longest_common_overlap(existing, new_text, 0.8)
    print(f"Existing: '{existing}'")
    print(f"New: '{new_text}'")
    print(f"Overlap position: {result}")
    print(f"New part: '{new_text[result:]}'")
    print()

def test_fuzzy_match():
    print("=== Testing Fuzzy Match (thing/thingy case) ===")
    existing = "I said the word thing"
    new_text = "thingy is what I meant"

    # Test with different thresholds
    for threshold in [0.6, 0.7, 0.8, 0.9]:
        result = find_longest_common_overlap(existing, new_text, threshold)
        print(f"Threshold {threshold}: overlap position {result}")
        if result > 0:
            print(f"  New part: '{new_text[result:]}'")
    print()

def test_similarity_ratios():
    print("=== Testing Similarity Ratios ===")
    test_pairs = [
        ("thing", "thingy"),
        ("hello", "hello"),
        ("test", "testing"),
        ("completely", "different"),
        ("word", "words"),
    ]

    for s1, s2 in test_pairs:
        ratio = calculate_similarity_ratio(s1, s2)
        print(f"'{s1}' vs '{s2}': {ratio:.3f}")
    print()

def test_realistic_scenario():
    print("=== Testing Realistic Scenario ===")
    existing = "I need to buy some groceries from the store"
    new_text = "store today because I'm running low on food"

    for threshold in [0.6, 0.7, 0.8, 0.9]:
        result = find_longest_common_overlap(existing, new_text, threshold)
        print(f"Threshold {threshold}: overlap position {result}")
        if result > 0:
            print(f"  New part: '{new_text[result:]}'")
    print()

if __name__ == "__main__":
    test_exact_match()
    test_fuzzy_match()
    test_similarity_ratios()
    test_realistic_scenario()