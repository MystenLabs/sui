// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Our names have a fixed style, which is created in the format `@org/app`.
/// Versioning can be used only on the RPC layer to determine a fixed package
/// version (e.g @org/app:v1)).
///
/// The only restrictions that apply to a label are:
/// - It must be up to 64 characters per label
/// - It can only contain alphanumeric characters, in lower case, and dashes
/// (singular, not in the beginning or end)
module move_registry::name;

use move_registry::domain::{Self, Domain};
use std::string::String;

/// A name format is `@org/app`
/// We keep "org" part flexible, in a future world where SuiNS subdomains could
/// also be nested.
/// So `example@org/app` would also be valid, and `inner.example@org/app` would
/// also be valid.
public struct Name has copy, store, drop {
    /// The ORG part of the name is a SuiNS Domain.
    org: Domain,
    /// The APP part of the name. We keep it as a vector, even though it'll
    /// always be a single element.
    /// That allows us to extend the name further in the future.
    app: vector<String>,
}

/// Creates a new `Name`.
public fun new(app: String, org: String): Name {
    // validate that our app is a valid label.
    validate_labels(&vector[app]);

    Name {
        org: domain::new(org),
        app: vector[app],
    }
}

public(package) fun validate_labels(labels: &vector<String>) {
    assert!(!labels.is_empty(), 0);

    labels.do_ref!(|label| assert!(is_valid_label(label), 0));
}

fun is_valid_label(label: &String): bool {
    let len = label.length();
    let label_bytes = label.as_bytes();
    let mut index = 0;

    if (len < 1 || len > 63) return false;

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
