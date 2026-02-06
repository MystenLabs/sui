// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    cynic_codegen::register_schema("sui")
        .from_sdl_file("../sui-indexer-alt-graphql/schema.graphql")
        .unwrap()
        .as_default()
        .unwrap();
}
