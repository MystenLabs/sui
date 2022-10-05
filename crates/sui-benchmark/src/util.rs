// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_sdk::crypto::FileBasedKeystore;
use sui_types::{
    base_types::SuiAddress,
    crypto::{AccountKeyPair, EncodeDecodeBase64, SuiKeyPair},
};

use std::path::PathBuf;

pub fn get_ed25519_keypair_from_keystore(
    keystore_path: PathBuf,
    requested_address: &SuiAddress,
) -> Result<AccountKeyPair> {
    let keystore = FileBasedKeystore::load_or_create(&keystore_path)?;
    let keypair = keystore
        .key_pairs()
        .iter()
        .find(|x| {
            let address: SuiAddress = Into::<SuiAddress>::into(&x.public());
            address == *requested_address
        })
        .map(|x| x.encode_base64())
        .unwrap();
    // TODO(joyqvq): This is a hack to decode base64 keypair with added flag, ok for now since it is for benchmark use.
    // Rework to get the typed keypair directly from above.
    Ok(match SuiKeyPair::decode_base64(&keypair).unwrap() {
        SuiKeyPair::Ed25519SuiKeyPair(x) => x,
        _ => panic!("Unexpected keypair type"),
    })
}
