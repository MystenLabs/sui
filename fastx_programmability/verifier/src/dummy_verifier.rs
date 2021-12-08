// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use fastpay_core::error::FastPayResult;
use move_binary_format::file_format::CompiledModule;

pub fn verify_module(_: &CompiledModule) -> FastPayResult {
    Ok(())
}
