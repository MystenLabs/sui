// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{errors::VMResult, file_format::CompiledModule};

pub fn verify_module(_: &CompiledModule) -> VMResult<()> {
    Ok(())
}
