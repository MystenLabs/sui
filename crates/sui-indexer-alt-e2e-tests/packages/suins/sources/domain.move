// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A mock of the SuiNS Domain type. It needs to be in its own module because
/// we hash its type off-chain, and while we can control which address we find
/// the package at, we don't control which module it is found in.
module suins::domain;

use std::string::String;

public struct Domain has copy, drop, store {
    labels: vector<String>,
}

public(package) fun new(labels: vector<String>): Domain {
    Domain { labels }
}
