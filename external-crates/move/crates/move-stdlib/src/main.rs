// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stdlib::utils::time_it;

#[tokio::main]
async fn main() {
    // Generate documentation
    {
        time_it("Generating stdlib documentation", async || {
            std::fs::remove_dir_all(move_stdlib::docs_full_path()).unwrap_or(());
            move_stdlib::build_doc(move_stdlib::docs_full_path())
                .await
                .unwrap();
        })
        .await;
    }
}
