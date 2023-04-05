// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_package::BuildConfig;
use std::path::Path;

#[test]
fn cross_thread_synchronization() {
    let handle = std::thread::spawn(|| {
        BuildConfig::default()
            .compile_package(
                Path::new("./tests/thread_safety_package_test_sources/Package1"),
                &mut std::io::stdout(),
            )
            .unwrap()
    });

    BuildConfig::default()
        .compile_package(
            Path::new("./tests/thread_safety_package_test_sources/Package2"),
            &mut std::io::stdout(),
        )
        .unwrap();
    handle.join().unwrap();
}
