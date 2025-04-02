// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    let schema_name = "rpc";
    sui_graphql_client_build::register_schema(schema_name);
}
