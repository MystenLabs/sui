// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_snapshot;

mod test {
    use super::*;

    #[test]
    fn test_schema_sdl_export() {
        let sdl = sui_graphql_rpc::schema_sdl_export();
        assert_snapshot!(sdl);
    }
}
