// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use clap::Parser;
use move_bytecode_viewer::BytecodeViewerConfig;

fn main() {
    BytecodeViewerConfig::parse().start_viewer()
}
