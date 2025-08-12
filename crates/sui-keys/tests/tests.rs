// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::str::FromStr;

use fastcrypto::hash::HashFunction;
use fastcrypto::traits::EncodeDecodeBase64;
use sui_keys::key_derive::generate_new_key;
use tempfile::TempDir;

use sui_keys::keystore::{
    AccountKeystore, Alias, FileBasedKeystore, GenerateOptions, GeneratedKey, InMemKeystore,
    Keystore, ALIASES_FILE_EXTENSION,
};
use sui_types::crypto::{DefaultHash, SignatureScheme, SuiSignatureInner};
use sui_types::{
    base_types::{SuiAddress, SUI_ADDRESS_LENGTH},
    crypto::Ed25519SuiSignature,
};

#[tokio::test]
async fn alias_exists_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    keystore
        .generate(
            Some("my_alias_test".to_string()),
            GenerateOptions::default(),
        )
        .await
        .unwrap();
    let aliases = alias_names(keystore.aliases());
    assert_eq!(1, aliases.len());
    assert_eq!(vec!["my_alias_test"], aliases);
    assert!(!aliases.contains(&"alias_does_not_exist"));
}

#[tokio::test]
async fn create_alias_keystore_file_test() {
    let temp_dir = TempDir::new().unwrap();
    let mut keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    keystore
        .generate(None, GenerateOptions::default())
        .await
        .unwrap();

    keystore_path.set_extension(ALIASES_FILE_EXTENSION);
    assert!(keystore_path.exists());

    keystore_path = temp_dir.path().join("myfile.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    keystore
        .generate(None, GenerateOptions::default())
        .await
        .unwrap();

    keystore_path.set_extension(ALIASES_FILE_EXTENSION);
    assert!(keystore_path.exists());
}

#[tokio::test]
async fn check_reading_aliases_file_correctly() {
    // when reading the alias file containing alias + public key base 64,
    // make sure the addresses are correctly converted back from pk

    let temp_dir = TempDir::new().unwrap();
    let mut keystore_path = temp_dir.path().join("sui.keystore");
    let keystore_path_keep = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    let GeneratedKey { address, .. } = keystore
        .generate(None, GenerateOptions::default())
        .await
        .unwrap();
    keystore_path.set_extension(ALIASES_FILE_EXTENSION);
    assert!(keystore_path.exists());

    let new_keystore =
        Keystore::from(FileBasedKeystore::load_or_create(&keystore_path_keep).unwrap());
    let addresses = new_keystore.addresses_with_alias();
    assert_eq!(address, *addresses.first().unwrap().0)
}

#[tokio::test]
async fn create_alias_if_not_exists_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());

    let alias = Some("my_alias_test".to_string());
    keystore
        .generate(alias.clone(), GenerateOptions::default())
        .await
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
    // this tests if when calling FileBasedKeystore::load_or_create(, it creates a
    // sui.aliases file with the existing address in the sui.keystore,
    // and a new alias for it.
    // This idea is to test the correct conversion
    // from the old type (which only contains keys and an optional path)
    // to the new type which contains keys and aliases (and an optional path), and if it creates the aliases file.

    let temp_dir = TempDir::new().unwrap();
    let mut keystore_path = temp_dir.path().join("sui.keystore");
    let (_, keypair, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let private_keys = vec![keypair.encode_base64()];
    let keystore_data = serde_json::to_string_pretty(&private_keys).unwrap();
    fs::write(&keystore_path, keystore_data).unwrap();

    let keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    keystore_path.set_extension(ALIASES_FILE_EXTENSION);
    assert!(keystore_path.exists());
    assert_eq!(1, keystore.aliases().len());
}

#[tokio::test]
async fn update_alias_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());

    keystore
        .generate(
            Some("my_alias_test".to_string()),
            GenerateOptions::default(),
        )
        .await
        .unwrap();

    let aliases = alias_names(keystore.aliases());
    assert_eq!(1, aliases.len());
    assert_eq!(vec!["my_alias_test"], aliases);

    // read the alias file again and check if it was saved
    let keystore1 = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    let aliases1 = alias_names(keystore1.aliases());
    assert_eq!(vec!["my_alias_test"], aliases1);

    let update = keystore.update_alias("alias_does_not_exist", None).await;
    assert!(update.is_err());

    let _ = keystore
        .update_alias("my_alias_test", Some("new_alias"))
        .await;
    let aliases = alias_names(keystore.aliases());
    assert_eq!(vec!["new_alias"], aliases);

    // check that it errors on empty alias
    assert!(keystore.update_alias("new_alias", Some(" ")).await.is_err());
    assert!(keystore
        .update_alias("new_alias", Some("   "))
        .await
        .is_err());
    // check that alias is trimmed
    assert!(keystore
        .update_alias("new_alias", Some("  o "))
        .await
        .is_ok());
    assert_eq!(vec!["o"], alias_names(keystore.aliases()));
    // check the regex works and new alias can be only [A-Za-z][A-Za-z0-9-_]*
    assert!(keystore.update_alias("o", Some("_alias")).await.is_err());
    assert!(keystore.update_alias("o", Some("-alias")).await.is_err());
    assert!(keystore.update_alias("o", Some("123")).await.is_err());

    let update = keystore.update_alias("o", None).await.unwrap();
    let aliases = alias_names(keystore.aliases());
    assert_eq!(vec![&update], aliases);

    // check that updating alias does not allow duplicates
    keystore
        .generate(
            Some("my_alias_test".to_string()),
            GenerateOptions::default(),
        )
        .await
        .unwrap();

    assert!(keystore
        .update_alias("my_alias_test", Some(&update))
        .await
        .is_err());
}

#[tokio::test]
async fn update_alias_in_memory_test() {
    let mut keystore = Keystore::InMem(InMemKeystore::new_insecure_for_tests(0));
    keystore
        .generate(
            Some("my_alias_test".to_string()),
            GenerateOptions::default(),
        )
        .await
        .unwrap();
    let aliases = alias_names(keystore.aliases());
    assert_eq!(1, aliases.len());
    assert_eq!(vec!["my_alias_test"], aliases);

    let update = keystore.update_alias("alias_does_not_exist", None).await;
    assert!(update.is_err());

    let _ = keystore
        .update_alias("my_alias_test", Some("new_alias"))
        .await;
    let aliases = alias_names(keystore.aliases());
    assert_eq!(vec!["new_alias"], aliases);

    let update = keystore.update_alias("new_alias", None).await.unwrap();
    let aliases = alias_names(keystore.aliases());
    assert_eq!(vec![&update], aliases);
}

#[tokio::test]
async fn mnemonic_test() {
    let temp_dir = TempDir::new().unwrap();
    let (address, _keypair, scheme, phrase) =
        generate_new_key(SignatureScheme::ED25519, None, None).unwrap();

    let keystore_path_2 = temp_dir.path().join("sui2.keystore");
    let mut keystore2 =
        Keystore::from(FileBasedKeystore::load_or_create(&keystore_path_2).unwrap());
    let imported_address = keystore2
        .import_from_mnemonic(&phrase, SignatureScheme::ED25519, None, None)
        .await
        .unwrap();
    assert_eq!(scheme.flag(), Ed25519SuiSignature::SCHEME.flag());
    assert_eq!(address, imported_address);
}

/// This test confirms rust's implementation of mnemonic is the same with the Sui Wallet
#[tokio::test]
async fn sui_wallet_address_mnemonic_test() -> Result<(), anyhow::Error> {
    let phrase = "result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss";
    let expected_address =
        SuiAddress::from_str("0x936accb491f0facaac668baaedcf4d0cfc6da1120b66f77fa6a43af718669973")?;

    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());

    keystore
        .import_from_mnemonic(phrase, SignatureScheme::ED25519, None, None)
        .await
        .unwrap();

    let pubkey = keystore.entries()[0].clone();
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
    let keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    assert!(keystore.to_string().contains("sui.keystore"));
    assert!(!keystore.to_string().contains("keys:"));
    Ok(())
}

#[tokio::test]
async fn get_alias_by_address_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
    let alias = "my_alias_test".to_string();
    let keypair = keystore
        .generate(Some(alias.clone()), GenerateOptions::default())
        .await
        .unwrap();
    assert_eq!(alias, keystore.get_alias(&keypair.address).unwrap());

    // Test getting an alias of an address that is not in keystore
    let address = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    assert!(keystore.get_alias(&address.0).is_err())
}

#[tokio::test]
async fn remove_key_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());

    let GeneratedKey { address, .. } = keystore
        .generate(Some("test_key".to_string()), GenerateOptions::default())
        .await
        .unwrap();

    let mut aliases_path = keystore_path.clone();
    aliases_path.set_extension(ALIASES_FILE_EXTENSION);

    let aliases_content = fs::read_to_string(&aliases_path).unwrap();
    assert!(aliases_content.contains("test_key"));

    keystore.remove(address).await.unwrap();

    // Verify alias is removed from file
    let aliases_content = fs::read_to_string(&aliases_path).unwrap();
    assert!(!aliases_content.contains("test_key"));
}

fn alias_names(aliases: Vec<&Alias>) -> Vec<&str> {
    aliases
        .into_iter()
        .map(|alias| alias.alias.as_str())
        .collect()
}
