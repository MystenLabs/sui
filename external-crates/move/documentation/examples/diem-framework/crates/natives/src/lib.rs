// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod account;
pub mod signature;

use std::sync::Arc;

use move_core_types::account_address::AccountAddress;
use move_vm_runtime::native_functions::{
    make_table_from_iter, NativeFunction, NativeFunctionTable,
};

pub fn all_natives(diem_framework_addr: AccountAddress) -> NativeFunctionTable {
    let natives: [(&str, &str, NativeFunction); 5] = [
        // TODO: Remove once/if DPN is moved over to use the core framework
        (
            "DiemAccount",
            "create_signer",
            Arc::new(account::native_create_signer),
        ),
        (
            "DiemAccount",
            "destroy_signer",
            Arc::new(account::native_destroy_signer),
        ),
        (
            "Signature",
            "ed25519_validate_pubkey",
            Arc::new(signature::native_ed25519_publickey_validation),
        ),
        (
            "Signature",
            "ed25519_verify",
            Arc::new(signature::native_ed25519_signature_verification),
        ),
        (
            "Account",
            "create_signer",
            Arc::new(account::native_create_signer),
        ),
    ];

    make_table_from_iter(diem_framework_addr, natives)
}
