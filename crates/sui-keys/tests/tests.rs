// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::str::FromStr;

use fastcrypto::hash::HashFunction;
use tempfile::TempDir;

use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_types::crypto::{DefaultHash, SignatureScheme, SuiSignatureInner};
use sui_types::{
    base_types::{SuiAddress, SUI_ADDRESS_LENGTH},
    crypto::Ed25519SuiSignature,
};
#[test]
fn mnemonic_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let (address, phrase, scheme) = keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None)
        .unwrap();

    let keystore_path_2 = temp_dir.path().join("sui2.keystore");
    let mut keystore2 = Keystore::from(FileBasedKeystore::new(&keystore_path_2).unwrap());
    let imported_address = keystore2
        .import_from_mnemonic(&phrase, SignatureScheme::ED25519, None)
        .unwrap();
    assert_eq!(scheme.flag(), Ed25519SuiSignature::SCHEME.flag());
    assert_eq!(address, imported_address);
}

/// This test confirms rust's implementation of mnemonic is the same with the Sui Wallet
#[test]
fn sui_wallet_address_mnemonic_test() -> Result<(), anyhow::Error> {
    let phrase = "result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss";
    let expected_address =
        SuiAddress::from_str("0x936accb491f0facaac668baaedcf4d0cfc6da1120b66f77fa6a43af718669973")?;

    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());

    keystore
        .import_from_mnemonic(phrase, SignatureScheme::ED25519, None)
        .unwrap();

    let pubkey = keystore.keys()[0].clone();
    assert_eq!(pubkey.flag(), Ed25519SuiSignature::SCHEME.flag());

    let mut hasher = DefaultHash::default();
    hasher.update([pubkey.flag()]);
    hasher.update(pubkey);
    let g_arr = hasher.finalize();
    let mut res = [0u8; SUI_ADDRESS_LENGTH];
    res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
    let address = SuiAddress::try_from(res.as_slice())?;

    assert_eq!(expected_address, address);

    Ok(())
}

#[test]
fn keystore_display_test() -> Result<(), anyhow::Error> {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    assert!(keystore.to_string().contains("sui.keystore"));
    assert!(!keystore.to_string().contains("keys:"));
    Ok(())
}
