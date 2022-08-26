// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::str::FromStr;

use sha3::{Digest, Sha3_256};
use tempfile::TempDir;

use sui_sdk::crypto::KeystoreType;
use sui_types::crypto::SuiSignatureInner;
use sui_types::{
    base_types::{SuiAddress, SUI_ADDRESS_LENGTH},
    crypto::Ed25519SuiSignature,
};
#[test]
fn mnemonic_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = KeystoreType::File(keystore_path).init().unwrap();

    let (address, phrase, flag) = keystore.generate_new_key(None).unwrap();

    let keystore_path_2 = temp_dir.path().join("sui2.keystore");
    let mut keystore2 = KeystoreType::File(keystore_path_2).init().unwrap();
    let imported_address = keystore2.import_from_mnemonic(&phrase, None).unwrap();
    assert_eq!(flag, Ed25519SuiSignature::SCHEME.flag());
    assert_eq!(address, imported_address);
}

/// This test confirms rust's implementation of mnemonic is the same with the Sui Wallet
#[test]
fn sui_wallet_address_mnemonic_test() -> Result<(), anyhow::Error> {
    // Recovery phase and SuiAddress obtained from Sui wallet v0.0.4 (prior key flag changes)
    let phrase = "oil puzzle immense upon pony govern jelly neck portion laptop laptop wall";
    let expected_address = SuiAddress::from_str("0x6a06dd564dfb2f0c71f3e167a48f569c705ed34c")?;

    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = KeystoreType::File(keystore_path).init().unwrap();

    keystore.import_from_mnemonic(phrase, None).unwrap();

    let pubkey = keystore.keys()[0].clone();
    assert_eq!(pubkey.flag(), Ed25519SuiSignature::SCHEME.flag());

    let mut hasher = Sha3_256::default();
    hasher.update(pubkey);
    let g_arr = hasher.finalize();
    let mut res = [0u8; SUI_ADDRESS_LENGTH];
    res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
    let address = SuiAddress::try_from(res.as_slice())?;

    assert_eq!(expected_address, address);

    Ok(())
}
