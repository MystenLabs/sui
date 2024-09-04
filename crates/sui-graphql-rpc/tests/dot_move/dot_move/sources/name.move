// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module dotmove::name {
    use std::string::String;

    const SEPARATOR: vector<u8> = b"@";

    public struct Name has copy, store, drop {
        // the labels of the label e.g. [org, example]. Keeping them reverse for consistency
        // with SuiNS
        labels: vector<String>,
        // The normalized version of the label (e.g. `example@org`)
        normalized: String
    }

    public fun new(name: String): Name {
        let labels = split_by_separator(name);

        Name {
            labels,
            normalized: name
        }
    }

    public(package) fun split_by_separator(mut name: String): vector<String> {
        let mut labels: vector<String> = vector[];
        let separator = SEPARATOR.to_string();

        while(!name.is_empty()) {
            let next_separator_index = name.index_of(&separator);
            let part = name.sub_string(0, next_separator_index);
            labels.push_back(part);

            let next_portion = if (next_separator_index == name.length()) {
                name.length()
            } else {
                next_separator_index + 1
            };

            name = name.sub_string(next_portion, name.length());
        };

        labels.reverse();
        labels
    }
}
