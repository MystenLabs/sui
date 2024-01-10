// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::str::FromStr;

use fastcrypto::hash::HashFunction;
use sui_keys::key_derive::generate_new_key;
use tempfile::TempDir;

use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, InMemKeystore, Keystore};
use sui_types::crypto::{DefaultHash, SignatureScheme, SuiSignatureInner};
use sui_types::{
    base_types::{SuiAddress, SUI_ADDRESS_LENGTH},
    crypto::Ed25519SuiSignature,
};

#[test]
fn alias_exists_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    keystore
        .generate_and_add_new_key(
            SignatureScheme::ED25519,
            Some("my_alias_test".to_string()),
            None,
            None,
        )
        .unwrap();
    let aliases = keystore.alias_names();
    assert_eq!(1, aliases.len());
    assert_eq!(vec!["my_alias_test"], aliases);
    assert!(!aliases.contains(&"alias_does_not_exist"));
}

#[test]
fn create_alias_keystore_file_test() {
    let temp_dir = TempDir::new().unwrap();
    let mut keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None, None)
        .unwrap();
    keystore_path.set_extension("aliases");
    assert!(keystore_path.exists());

    keystore_path = temp_dir.path().join("myfile.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None, None)
        .unwrap();
    keystore_path.set_extension("aliases");
    assert!(keystore_path.exists());
}

#[test]
fn check_reading_aliases_file_correctly() {
    // when reading the alias file containing alias + public key base 64,
    // make sure the addresses are correctly converted back from pk

    let temp_dir = TempDir::new().unwrap();
    let mut keystore_path = temp_dir.path().join("sui.keystore");
    let keystore_path_keep = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let kp = keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None, None)
        .unwrap();
    keystore_path.set_extension("aliases");
    assert!(keystore_path.exists());

    let new_keystore = Keystore::from(FileBasedKeystore::new(&keystore_path_keep).unwrap());
    let addresses = new_keystore.addresses_with_alias();
    assert_eq!(kp.0, *addresses.get(0).unwrap().0)
}

#[test]
fn create_alias_if_not_exists_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());

    let alias = Some("my_alias_test".to_string());
    keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, alias.clone(), None, None)
        .unwrap();

    // test error first
    let create_alias_result = keystore.create_alias(alias);
    assert!(create_alias_result.is_err());
    // test expected result
    let create_alias_result = keystore.create_alias(Some("test".to_string()));
    assert_eq!("test".to_string(), create_alias_result.unwrap());
    assert!(keystore.create_alias(Some("_test".to_string())).is_err());
    assert!(keystore.create_alias(Some("-A".to_string())).is_err());
    assert!(keystore.create_alias(Some("1A".to_string())).is_err());
    assert!(keystore.create_alias(Some("&&AA".to_string())).is_err());
}

#[test]
fn keystore_no_aliases() {
    // this tests if when calling FileBasedKeystore::new, it creates a
    // sui.aliases file with the existing address in the sui.keystore,
    // and a new alias for it.
    // This idea is to test the correct conversion
    // from the old type (which only contains keys and an optional path)
    // to the new type which contains keys and aliases (and an optional path), and if it creates the aliases file.

    let temp_dir = TempDir::new().unwrap();
    let mut keystore_path = temp_dir.path().join("sui.keystore");
    let (_, keypair, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let private_keys = vec![keypair.encode().unwrap()];
    let keystore_data = serde_json::to_string_pretty(&private_keys).unwrap();
    fs::write(&keystore_path, keystore_data).unwrap();

    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    keystore_path.set_extension("aliases");
    assert!(keystore_path.exists());
    assert_eq!(1, keystore.aliases().len());
}

#[test]
fn update_alias_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    keystore
        .generate_and_add_new_key(
            SignatureScheme::ED25519,
            Some("my_alias_test".to_string()),
            None,
            None,
        )
        .unwrap();
    let aliases = keystore.alias_names();
    assert_eq!(1, aliases.len());
    assert_eq!(vec!["my_alias_test"], aliases);

    // read the alias file again and check if it was saved
    let keystore1 = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let aliases1 = keystore1.alias_names();
    assert_eq!(vec!["my_alias_test"], aliases1);

    let update = keystore.update_alias("alias_does_not_exist", None);
    assert!(update.is_err());

    let _ = keystore.update_alias("my_alias_test", Some("new_alias"));
    let aliases = keystore.alias_names();
    assert_eq!(vec!["new_alias"], aliases);

    // check that it errors on empty alias
    assert!(keystore.update_alias("new_alias", Some(" ")).is_err());
    assert!(keystore.update_alias("new_alias", Some("   ")).is_err());
    // check that alias is trimmed
    assert!(keystore.update_alias("new_alias", Some("  o ")).is_ok());
    assert_eq!(vec!["o"], keystore.alias_names());
    // check the regex works and new alias can be only [A-Za-z][A-Za-z0-9-_]*
    assert!(keystore.update_alias("o", Some("_alias")).is_err());
    assert!(keystore.update_alias("o", Some("-alias")).is_err());
    assert!(keystore.update_alias("o", Some("123")).is_err());

    let update = keystore.update_alias("o", None).unwrap();
    let aliases = keystore.alias_names();
    assert_eq!(vec![&update], aliases);
}

#[test]
fn update_alias_in_memory_test() {
    let mut keystore = Keystore::InMem(InMemKeystore::new_insecure_for_tests(0));
    keystore
        .generate_and_add_new_key(
            SignatureScheme::ED25519,
            Some("my_alias_test".to_string()),
            None,
            None,
        )
        .unwrap();
    let aliases = keystore.alias_names();
    assert_eq!(1, aliases.len());
    assert_eq!(vec!["my_alias_test"], aliases);

    let update = keystore.update_alias("alias_does_not_exist", None);
    assert!(update.is_err());

    let _ = keystore.update_alias("my_alias_test", Some("new_alias"));
    let aliases = keystore.alias_names();
    assert_eq!(vec!["new_alias"], aliases);

    let update = keystore.update_alias("new_alias", None).unwrap();
    let aliases = keystore.alias_names();
    assert_eq!(vec![&update], aliases);
}

#[test]
fn mnemonic_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let (address, phrase, scheme) = keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None, None)
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

#[test]
fn get_alias_by_address_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let alias = "my_alias_test".to_string();
    let keypair = keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, Some(alias.clone()), None, None)
        .unwrap();
    assert_eq!(alias, keystore.get_alias_by_address(&keypair.0).unwrap());

    // Test getting an alias of an address that is not in keystore
    let address = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    assert!(keystore.get_alias_by_address(&address.0).is_err())
}
