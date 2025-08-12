// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{build_config::BuildConfig, build_plan::BuildPlan, compiled_package::CompiledPackage};
use move_package_alt::{
    errors::PackageResult, flavor::MoveFlavor, package::RootPackage, schema::Environment,
};
use std::{io::Write, path::Path};

pub mod build_config;
pub mod build_plan;
pub mod compiled_package;
pub mod layout;
pub mod lint_flag;
pub mod model_builder;
pub mod on_disk_package;

pub async fn compile_package<W: Write, F: MoveFlavor>(
    path: &Path,
    build_config: &BuildConfig,
    env: &Environment,
    writer: &mut W,
) -> PackageResult<CompiledPackage> {
    let root_pkg = RootPackage::<F>::load(path, env.clone()).await?;
    BuildPlan::create(root_pkg, build_config)?.compile(writer, |compiler| compiler)
}

pub async fn compile_from_root_package<W: Write, F: MoveFlavor>(
    root_pkg: RootPackage<F>,
    build_config: &BuildConfig,
    writer: &mut W,
) -> PackageResult<CompiledPackage> {
    BuildPlan::create(root_pkg, build_config)?.compile(writer, |compiler| compiler)
}
