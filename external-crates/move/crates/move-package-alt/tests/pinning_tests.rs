// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_command_line_common::insta_assert;
use move_package_alt::{
    dependency::{self, DependencySet, ManifestDependencyInfo},
    flavor::Vanilla,
    package::manifest::Manifest,
};

async fn run_pinning_tests(input_path: &Path) -> datatest_stable::Result<()> {
    let manifest = Manifest::<Vanilla>::read_from(input_path).unwrap();

    let deps: DependencySet<ManifestDependencyInfo<Vanilla>> = manifest
        .dependencies
        .into_iter()
        .map(|(package, dep)| (None, package, dep.dependency_info))
        .collect();

    let pinned = dependency::pin(&Vanilla, &deps, &manifest.environments).await?;

    insta_assert! {
        input_path: input_path,
        contents: format!("{:?}", pinned.default_deps()),
        suffix: "pinned",
    }

    Ok(())
}

fn run_pinning_wrapper(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_pinning_tests(path))?;
    Ok(())
}

datatest_stable::harness!(run_pinning_wrapper, "tests/data", r"pinning.*\.toml$",);
