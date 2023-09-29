// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_snapshot;

mod test {
    use super::*;
    use std::fs::write;
    use std::path::PathBuf;

    #[test]
    fn test_schema_sdl_export() {
        let sdl = sui_graphql_rpc::schema_sdl_export();
        assert_snapshot!(sdl);

        // update the current schema file
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("schema/current_progress_schema.graphql");
        write(path, sdl).unwrap();
    }
}
