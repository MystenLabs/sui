// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub fn main() -> anyhow::Result<()> {
    move_bytecode_verifier_analyzer::analyzer::run()
}
