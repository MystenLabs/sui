// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

pub mod adapter;
mod state_view;

use move_core_types::account_address::AccountAddress;

/// Extract + return an address from the first `authenticator.length()` bytes of `object`.
/// Replace theses bytes with `authenticator`.
/// copy the first authenticator.length() bytes out of `object`, turn them into
/// an address. and return them. then, replace the first authenicator.length()
/// bytes of `object` with `authenticator`
pub(crate) fn swap_authenticator_and_id(
    authenticator: AccountAddress,
    object: &mut Vec<u8>,
) -> AccountAddress {
    assert!(object.len() > authenticator.len());

    let authenticator_bytes = authenticator.into_bytes();
    let mut id_bytes = [0u8; AccountAddress::LENGTH];

    id_bytes[..authenticator.len()].clone_from_slice(&object[..authenticator.len()]);
    object[..authenticator.len()].clone_from_slice(&authenticator_bytes[..authenticator.len()]);
    AccountAddress::new(id_bytes)
}
