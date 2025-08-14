// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod build_config;
pub mod build_plan;
pub mod compilation;
pub mod compiled_package;
pub mod documentation;
pub mod layout;
pub mod lint_flag;
pub mod model_builder;
pub mod on_disk_package;
pub mod source_discovery;

pub use compilation::compile_from_root_package;
pub use compilation::compile_package;
