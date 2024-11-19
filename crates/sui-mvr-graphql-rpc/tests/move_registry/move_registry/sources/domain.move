// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_registry::domain;

use std::string::{String, utf8};

/// Representation of a valid SuiNS `Domain`.
public struct Domain has copy, drop, store {
    /// Vector of labels that make up a domain.
    ///
    /// Labels are stored in reverse order such that the TLD is always in
    /// position `0`.
    /// e.g. domain "pay.name.sui" will be stored in the vector as ["sui",
    /// "name", "pay"].
    labels: vector<String>,
}

// Construct a `Domain` by parsing and validating the provided string
public fun new(domain: String): Domain {
    assert!(domain.length() <= 235, 0);

    let mut labels = split_by_dot(domain);
    validate_labels(&labels);
    labels.reverse();
    Domain {
        labels,
    }
}

fun validate_labels(labels: &vector<String>) {
    assert!(!labels.is_empty(), 0);

    let len = labels.length();
    let mut index = 0;

    while (index < len) {
        let label = &labels[index];
        assert!(is_valid_label(label), 0);
        index = index + 1;
    }
}

fun is_valid_label(label: &String): bool {
    let len = label.length();
    let label_bytes = label.as_bytes();
    let mut index = 0;

    if (!(len >= 1 && len <= 63)) {
        return false
    };

    while (index < len) {
        let character = label_bytes[index];
        let is_valid_character =
            (0x61 <= character && character <= 0x7A)                   // a-z
                || (0x30 <= character && character <= 0x39)                // 0-9
                || (character == 0x2D && index != 0 && index != len - 1); // '-' not at beginning or end

        if (!is_valid_character) {
            return false
        };

        index = index + 1;
    };

    true
}

/// Splits a string `s` by the character `.` into a vector of subslices,
/// excluding the `.`
fun split_by_dot(mut s: String): vector<String> {
    let dot = utf8(b".");
    let mut parts: vector<String> = vector[];
    while (!s.is_empty()) {
        let index_of_next_dot = s.index_of(&dot);
        let part = s.substring(0, index_of_next_dot);
        parts.push_back(part);

        let len = s.length();
        let start_of_next_part = if (index_of_next_dot == len) {
            len
        } else {
            index_of_next_dot + 1
        };

        s = s.substring(start_of_next_part, len);
    };

    parts
}
