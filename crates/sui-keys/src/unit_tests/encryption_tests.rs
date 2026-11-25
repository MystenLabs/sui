// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use fastcrypto::encoding::{Base64, Encoding};
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{EncodeDecodeBase64, SignatureScheme, SuiKeyPair, SuiSignature};
use tempfile::TempDir;
use zeroize::Zeroizing;

use super::*;
use crate::key_derive::generate_new_key;
use crate::keystore::{
    ALIASES_FILE_EXTENSION, AccountKeystore, FileBasedKeystore, InMemKeystore, Keystore,
    recovery_copy_path,
};

/// Argon2 costs that keep the tests fast. Real keystores use `Argon2Cost::RECOMMENDED`.
const TEST_COST: Argon2Cost = Argon2Cost {
    memory_kib: 64,
    iterations: 1,
    parallelism: 1,
};
const PASSWORD: &str = "correct horse battery staple";
const KEYSTORE_PATH: &str = "/home/user/.sui/sui_config/sui.keystore";

/// Returns one new keypair of each signature scheme that a keystore can hold.
fn test_keys() -> BTreeMap<SuiAddress, SuiKeyPair> {
    [
        SignatureScheme::ED25519,
        SignatureScheme::Secp256k1,
        SignatureScheme::Secp256r1,
        SignatureScheme::MLDSA65,
    ]
    .into_iter()
    .map(|scheme| {
        let (address, key, _, _) = generate_new_key(scheme, None, None).unwrap();
        (address, key)
    })
    .collect()
}

fn encrypt_test_keys() -> (BTreeMap<SuiAddress, SuiKeyPair>, EncryptedKeystoreFile) {
    let keys = test_keys();
    let file = KeystoreKey::generate(PASSWORD, TEST_COST)
        .unwrap()
        .encrypt(&keys)
        .unwrap();
    (keys, file)
}

fn unlock_error(result: anyhow::Result<UnlockedKeys>) -> String {
    match result {
        Ok(_) => panic!("The keystore was unlocked"),
        Err(e) => e.to_string(),
    }
}

/// Creates a plaintext keystore file that holds the test keys.
async fn plaintext_keystore(dir: &TempDir) -> (PathBuf, BTreeMap<SuiAddress, SuiKeyPair>) {
    let path = dir.path().join("sui.keystore");
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    let keys = test_keys();
    for key in keys.values() {
        keystore.import(None, key.copy()).await.unwrap();
    }
    (path, keys)
}

/// Creates an encrypted keystore file that holds the test keys.
async fn encrypted_keystore(dir: &TempDir) -> (PathBuf, BTreeMap<SuiAddress, SuiKeyPair>) {
    let (path, keys) = plaintext_keystore(dir).await;
    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    (path, keys)
}

fn aliases(keystore: &FileBasedKeystore) -> Vec<(SuiAddress, String)> {
    keystore
        .addresses_with_alias()
        .into_iter()
        .map(|(address, alias)| (*address, alias.alias.clone()))
        .collect()
}

#[test]
fn decrypts_with_the_password() {
    let (keys, file) = encrypt_test_keys();
    let unlocked = decrypt_with_password(&file, PASSWORD).unwrap().unwrap();
    assert_eq!(unlocked.keys, keys);
}

#[test]
fn does_not_decrypt_with_another_password() {
    let (_, file) = encrypt_test_keys();
    assert!(
        decrypt_with_password(&file, "not the password")
            .unwrap()
            .is_none()
    );
}

#[test]
fn stores_public_keys_but_not_private_keys() {
    let (keys, file) = encrypt_test_keys();
    let json = serde_json::to_string(&file).unwrap();
    for key in keys.values() {
        assert!(json.contains(&key.public().encode_base64()));
        assert!(!json.contains(&key.encode_base64()));
        assert!(!json.contains(&key.encode().unwrap()));
    }
    let public_keys: Vec<_> = keys.values().map(|key| key.public()).collect();
    assert_eq!(file.public_keys().unwrap(), public_keys);
}

#[test]
fn encrypts_with_a_new_nonce_and_derives_with_a_new_salt() {
    let keys = test_keys();
    let key = KeystoreKey::generate(PASSWORD, TEST_COST).unwrap();
    let first = key.encrypt(&keys).unwrap();
    let second = key.encrypt(&keys).unwrap();
    assert_eq!(first.kdf, second.kdf);
    assert_ne!(first.cipher.nonce, second.cipher.nonce);
    assert_ne!(first.ciphertext, second.ciphertext);

    let other_key = KeystoreKey::generate(PASSWORD, TEST_COST).unwrap();
    assert_ne!(key.kdf.salt, other_key.kdf.salt);
    assert_ne!(*key.key, *other_key.key);
}

#[test]
fn does_not_decrypt_a_modified_file() {
    let (_, file) = encrypt_test_keys();
    let other_public_key = generate_new_key(SignatureScheme::ED25519, None, None)
        .unwrap()
        .1
        .public()
        .encode_base64();
    let mut flipped_ciphertext = Base64::decode(&file.ciphertext).unwrap();
    flipped_ciphertext[0] ^= 1;

    let assert_does_not_decrypt =
        |modification: &str, modify: &dyn Fn(&mut EncryptedKeystoreFile)| {
            let mut modified = file.clone();
            modify(&mut modified);
            assert!(
                !matches!(decrypt_with_password(&modified, PASSWORD), Ok(Some(_))),
                "The file with a {modification} was decrypted"
            );
        };
    assert_does_not_decrypt("replaced public key", &|file| {
        file.public_keys[0] = other_public_key.clone()
    });
    assert_does_not_decrypt("removed public key", &|file| {
        file.public_keys.pop();
    });
    assert_does_not_decrypt("reordered public keys", &|file| file.public_keys.reverse());
    assert_does_not_decrypt("changed iterations", &|file| file.kdf.iterations += 1);
    assert_does_not_decrypt("changed salt", &|file| {
        file.kdf.salt = Base64::encode([7u8; SALT_LENGTH])
    });
    assert_does_not_decrypt("changed nonce", &|file| {
        file.cipher.nonce = Base64::encode([7u8; NONCE_LENGTH])
    });
    assert_does_not_decrypt("changed ciphertext", &|file| {
        file.ciphertext = Base64::encode(&flipped_ciphertext)
    });
}

#[test]
fn parses_and_validates_the_file() {
    let (keys, file) = encrypt_test_keys();
    let json = serde_json::to_value(&file).unwrap();
    let (parsed, _) =
        EncryptedKeystoreFile::from_slice(&serde_json::to_vec(&json).unwrap()).unwrap();
    let unlocked = decrypt_with_password(&parsed, PASSWORD).unwrap().unwrap();
    assert_eq!(unlocked.keys, keys);

    let rejected = |modify: &dyn Fn(&mut serde_json::Value), expected: &str| {
        let mut json = json.clone();
        modify(&mut json);
        let contents = serde_json::to_vec(&json).unwrap();
        let error = format!(
            "{:#}",
            EncryptedKeystoreFile::from_slice(&contents).unwrap_err()
        );
        assert!(
            error.contains(expected),
            "`{error}` does not contain `{expected}`"
        );
    };
    rejected(&|json| json["version"] = 2.into(), "format version 2");
    rejected(
        &|json| {
            json.as_object_mut().unwrap().remove("version");
        },
        "no format version",
    );
    rejected(
        &|json| json["kdf"]["algorithm"] = "scrypt".into(),
        "key derivation function scrypt",
    );
    rejected(
        &|json| json["kdf"]["version"] = 16.into(),
        "Argon2 version 16",
    );
    rejected(
        &|json| json["kdf"]["memory_kib"] = (MAX_MEMORY_KIB + 1).into(),
        "memory cost",
    );
    rejected(
        &|json| json["kdf"]["iterations"] = (MAX_ITERATIONS + 1).into(),
        "iteration count",
    );
    rejected(
        &|json| json["kdf"]["parallelism"] = (MAX_PARALLELISM + 1).into(),
        "parallelism",
    );
    rejected(
        &|json| json["kdf"]["iterations"] = 0.into(),
        "Invalid Argon2 parameters",
    );
    rejected(
        &|json| json["kdf"]["salt"] = Base64::encode([0u8; 8]).into(),
        "salt is shorter",
    );
    rejected(
        &|json| json["cipher"]["algorithm"] = "aes-128-gcm".into(),
        "cipher aes-128-gcm",
    );
    rejected(
        &|json| json["cipher"]["nonce"] = Base64::encode([0u8; 16]).into(),
        "nonce must be 12 bytes",
    );
    rejected(
        &|json| json["ciphertext"] = "not base64!".into(),
        "Invalid ciphertext",
    );
    rejected(
        &|json| json["ciphertext"] = Base64::encode([0u8; TAG_LENGTH - 1]).into(),
        "shorter than the 16-byte authentication tag",
    );
    rejected(
        &|json| json["public_keys"][0] = "not a public key".into(),
        "Invalid public key",
    );
    rejected(
        &|json| {
            let first = json["public_keys"][0].clone();
            json["public_keys"][1] = first;
        },
        "lists a public key more than once",
    );
    rejected(&|json| json["extra"] = true.into(), "unknown field");
    rejected(
        &|json| {
            json.as_object_mut().unwrap().remove("ciphertext");
        },
        "missing field",
    );
}

/// A file in format version 1, with keys whose private key bytes are all 1 (Ed25519) and all 2
/// (Secp256k1), encrypted with `PASSWORD`. The encryption authenticates fields in an encoding that
/// the file does not store, so only a file written by an earlier build detects a change to it.
const VERSION_1_FILE: &str = r#"{
  "version": 1,
  "public_keys": [
    "AIqI4910CfGV/VLbLTy6XXLKZwm/HZQSG/N0iAG0D29c",
    "AQJNS2zRNhAyypvSrrnZAKpNRdnq2ArJQjN0xFGnJU0HZg=="
  ],
  "kdf": {
    "algorithm": "argon2id",
    "version": 19,
    "memory_kib": 8,
    "iterations": 1,
    "parallelism": 1,
    "salt": "HfjaEqPnpyNZj/ss436W+g=="
  },
  "cipher": {
    "algorithm": "aes-256-gcm",
    "nonce": "wnWschitUkfI2WJz"
  },
  "ciphertext": "bsLMH2nTBvGswa6lwDR8pO1sU3iqnoi70uCNAPKSsrXcIyeRCQwb84wpGN8m0/8sI1Fkwz8gAzAhwGF9UeHjK/FzNC9FT/E9VhHlAFc2d7dsWIRhifmazdquAdjYybxkxVOs5csISxy4Mg7SbdU899FajqZUUw=="
}"#;

#[test]
fn decrypts_a_file_in_format_version_1() {
    let (file, _) = EncryptedKeystoreFile::from_slice(VERSION_1_FILE.as_bytes()).unwrap();
    let unlocked = decrypt_with_password(&file, PASSWORD).unwrap().unwrap();
    let keys: BTreeSet<String> = unlocked
        .keys
        .values()
        .map(|key| key.encode_base64())
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "AAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEB".to_string(),
            "AQICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIC".to_string(),
        ])
    );
}

#[test]
fn rejects_an_empty_password() {
    let error = KeystoreKey::generate("", TEST_COST).err().unwrap();
    assert!(error.to_string().contains("cannot be empty"));
}

#[test]
fn recommended_cost_is_valid() {
    assert!(
        KdfParams::generate(Argon2Cost::RECOMMENDED)
            .argon2()
            .is_ok()
    );
}

#[test]
fn unlocks_with_the_password_from_the_environment() {
    let (keys, file) = encrypt_test_keys();
    let prompt =
        |_: &str| -> anyhow::Result<Option<String>> { panic!("The password prompt was used") };
    let unlocked = unlock_with(
        &file,
        Path::new(KEYSTORE_PATH),
        Some(Zeroizing::new(PASSWORD.to_string())),
        Some(&prompt),
    )
    .unwrap();
    assert_eq!(unlocked.keys, keys);

    let error = unlock_error(unlock_with(
        &file,
        Path::new(KEYSTORE_PATH),
        Some(Zeroizing::new("not the password".to_string())),
        Some(&prompt),
    ));
    assert!(error.contains(KEYSTORE_PASSWORD_ENV_VAR), "{error}");
    assert!(error.contains(KEYSTORE_PATH), "{error}");
}

#[test]
fn does_not_unlock_without_a_password_source() {
    let (_, file) = encrypt_test_keys();
    let error = unlock_error(unlock_with(&file, Path::new(KEYSTORE_PATH), None, None));
    assert!(error.contains("is encrypted"), "{error}");
    assert!(error.contains(KEYSTORE_PASSWORD_ENV_VAR), "{error}");
    // Without a prompt, a terminal would not help.
    assert!(!error.contains("terminal"), "{error}");

    let cannot_prompt = |_: &str| -> anyhow::Result<Option<String>> { Ok(None) };
    let error = unlock_error(unlock_with(
        &file,
        Path::new(KEYSTORE_PATH),
        None,
        Some(&cannot_prompt),
    ));
    assert!(error.contains("is encrypted"), "{error}");
    assert!(error.contains("interactive terminal"), "{error}");
}

#[test]
fn prompts_again_after_an_incorrect_password() {
    let (keys, file) = encrypt_test_keys();
    let attempts = Cell::new(0);
    let prompt = |message: &str| -> anyhow::Result<Option<String>> {
        assert!(message.contains(KEYSTORE_PATH), "{message}");
        attempts.set(attempts.get() + 1);
        let password = if attempts.get() < MAX_PROMPT_ATTEMPTS {
            "not the password"
        } else {
            PASSWORD
        };
        Ok(Some(password.to_string()))
    };
    let unlocked = unlock_with(&file, Path::new(KEYSTORE_PATH), None, Some(&prompt)).unwrap();
    assert_eq!(unlocked.keys, keys);
    assert_eq!(attempts.get(), MAX_PROMPT_ATTEMPTS);
}

#[test]
fn stops_prompting_after_the_maximum_number_of_attempts() {
    let (_, file) = encrypt_test_keys();
    let attempts = Cell::new(0);
    let prompt = |_: &str| -> anyhow::Result<Option<String>> {
        attempts.set(attempts.get() + 1);
        Ok(Some("not the password".to_string()))
    };
    let error = unlock_error(unlock_with(
        &file,
        Path::new(KEYSTORE_PATH),
        None,
        Some(&prompt),
    ));
    assert!(error.contains("password is incorrect"), "{error}");
    assert_eq!(attempts.get(), MAX_PROMPT_ATTEMPTS);
}

#[tokio::test]
async fn encrypts_a_keystore_and_unlocks_it_after_reloading() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = plaintext_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    assert!(keystore.is_encrypted());
    for (address, key) in &keys {
        assert_eq!(keystore.export(address).unwrap(), key);
    }

    let contents = fs::read_to_string(&path).unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&contents)
            .unwrap()
            .is_object()
    );
    for key in keys.values() {
        assert!(!contents.contains(&key.encode_base64()));
    }

    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(reloaded.is_encrypted());
    assert_eq!(reloaded.addresses(), keystore.addresses());
    assert_eq!(aliases(&reloaded), aliases(&keystore));
    assert!(reloaded.unlock("not the password").is_err());
    reloaded.unlock(PASSWORD).unwrap();
    // An unlocked keystore still checks the password.
    assert!(reloaded.unlock("not the password").is_err());
    reloaded.unlock(PASSWORD).unwrap();
    let message = PersonalMessage {
        message: b"Hello, Sui".to_vec(),
    };
    for (address, key) in &keys {
        assert_eq!(reloaded.export(address).unwrap(), key);
        let signature = reloaded
            .sign_secure(address, &message, Intent::personal_message())
            .await
            .unwrap();
        signature
            .verify_secure(
                &IntentMessage::new(Intent::personal_message(), message.clone()),
                *address,
                key.public().scheme(),
            )
            .unwrap();
    }
}

#[tokio::test]
async fn locked_keystore_updates_aliases_without_the_password() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let encrypted_contents = fs::read(&path).unwrap();

    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    let old_alias = keystore.aliases()[0].alias.clone();
    keystore
        .update_alias(&old_alias, Some("renamed"))
        .await
        .unwrap();
    keystore.save().await.unwrap();
    assert_eq!(fs::read(&path).unwrap(), encrypted_contents);

    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(reloaded.alias_exists("renamed"));
    assert!(!reloaded.alias_exists(&old_alias));
    reloaded.unlock(PASSWORD).unwrap();
}

#[tokio::test]
async fn import_and_remove_keep_the_keystore_encrypted() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = encrypted_keystore(&dir).await;

    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    let (new_address, new_key, _, _) =
        generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    keystore
        .import(Some("new-key".to_string()), new_key.copy())
        .await
        .unwrap();

    let mut reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(reloaded.is_encrypted());
    assert_eq!(reloaded.addresses().len(), keys.len() + 1);
    assert_eq!(reloaded.get_alias(&new_address).unwrap(), "new-key");
    reloaded.unlock(PASSWORD).unwrap();
    assert_eq!(reloaded.export(&new_address).unwrap(), &new_key);

    let removed = *keys.keys().next().unwrap();
    reloaded.remove(removed).await.unwrap();
    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(reloaded.is_encrypted());
    assert!(!reloaded.addresses().contains(&removed));
    assert!(reloaded.get_alias(&removed).is_err());
    reloaded.unlock(PASSWORD).unwrap();
    assert_eq!(reloaded.key_pairs().unwrap().len(), keys.len());
}

#[tokio::test]
async fn the_keystore_enum_unlocks_a_file_based_keystore() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let keystore = Keystore::from(FileBasedKeystore::load_or_create(&path).unwrap());

    // The test process registers no password prompt, and SUI_KEYSTORE_PASSWORD does not hold the
    // password of this keystore, so unlocking fails.
    assert!(keystore.ensure_unlocked().await.is_err());
    let Keystore::File(file_keystore) = &keystore else {
        panic!("The keystore is not file-based");
    };
    file_keystore.unlock(PASSWORD).unwrap();
    keystore.ensure_unlocked().await.unwrap();

    Keystore::from(InMemKeystore::new_insecure_for_tests(1))
        .ensure_unlocked()
        .await
        .unwrap();
}

#[tokio::test]
async fn unknown_addresses_do_not_unlock_the_keystore() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    let (unknown, _, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();

    let error = keystore.export(&unknown).unwrap_err();
    assert!(error.to_string().contains("Cannot find key"), "{error}");
    let message = PersonalMessage { message: vec![] };
    let error = keystore
        .sign_secure(&unknown, &message, Intent::personal_message())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Cannot find key"), "{error}");
    keystore.remove(unknown).await.unwrap();
    assert!(
        FileBasedKeystore::load_or_create(&path)
            .unwrap()
            .unlock(PASSWORD)
            .is_ok()
    );
}

#[tokio::test]
async fn changes_the_password() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = encrypted_keystore(&dir).await;
    let old_contents: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    keystore
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap();

    let new_contents: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_ne!(old_contents["kdf"]["salt"], new_contents["kdf"]["salt"]);
    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(reloaded.unlock(PASSWORD).is_err());
    reloaded.unlock("new password").unwrap();
    for (address, key) in &keys {
        assert_eq!(reloaded.export(address).unwrap(), key);
    }
}

#[tokio::test]
async fn decrypts_a_keystore() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = encrypted_keystore(&dir).await;

    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    keystore.decrypt().await.unwrap();
    assert!(!keystore.is_encrypted());

    let contents: Vec<String> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(contents.len(), keys.len());
    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(!reloaded.is_encrypted());
    for (address, key) in &keys {
        assert_eq!(reloaded.export(address).unwrap(), key);
    }
}

#[tokio::test]
async fn conversions_check_the_keystore_format() {
    let dir = TempDir::new().unwrap();
    let (path, _) = plaintext_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    let error = keystore.decrypt().await.unwrap_err();
    assert!(error.to_string().contains("not encrypted"), "{error}");
    let error = keystore
        .change_password_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not encrypted"), "{error}");
    let error = keystore.encrypt_with_cost("", TEST_COST).await.unwrap_err();
    assert!(error.to_string().contains("cannot be empty"), "{error}");
    assert!(
        !FileBasedKeystore::load_or_create(&path)
            .unwrap()
            .is_encrypted()
    );

    keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    let error = keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("already encrypted"), "{error}");

    let error = FileBasedKeystore::default()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no file"), "{error}");
}

#[tokio::test]
async fn rejects_an_invalid_encrypted_keystore_file() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json["public_keys"][0] = "not a key".into();
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let error = format!(
        "{:#}",
        FileBasedKeystore::load_or_create(&path).err().unwrap()
    );
    assert!(error.contains("Invalid encrypted keystore file"), "{error}");
    assert!(error.contains("Invalid public key"), "{error}");

    // A client config reads its keystore from the path that it stores, and its error also names
    // the cause.
    let error = serde_json::from_value::<FileBasedKeystore>(path.to_str().unwrap().into())
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("Invalid public key"), "{error}");
}

/// Adds a new key to the encrypted keystore at `path`, as another process would.
async fn import_from_another_process(path: &PathBuf) -> SuiAddress {
    let mut other = FileBasedKeystore::load_or_create(path).unwrap();
    other.unlock(PASSWORD).unwrap();
    let (address, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    other.import(None, key).await.unwrap();
    address
}

#[tokio::test]
async fn writes_fail_if_another_process_changed_the_keystore() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = encrypted_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    let other_address = import_from_another_process(&path).await;

    // Signing still works, with the keys that this keystore read.
    keystore.unlock(PASSWORD).unwrap();
    let message = PersonalMessage {
        message: b"Hello, Sui".to_vec(),
    };
    let address = *keys.keys().next().unwrap();
    keystore
        .sign_secure(&address, &message, Intent::personal_message())
        .await
        .unwrap();

    let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let error = keystore.import(None, key).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed after this command read it"),
        "{error}"
    );
    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert_eq!(reloaded.addresses().len(), keys.len() + 1);
    assert!(reloaded.addresses().contains(&other_address));
    // The refused save did not write the aliases either.
    assert!(reloaded.get_alias(&other_address).is_ok());
}

#[tokio::test]
async fn alias_updates_fail_if_another_process_changed_the_keystore() {
    for encrypted in [false, true] {
        let dir = TempDir::new().unwrap();
        let (path, _) = if encrypted {
            encrypted_keystore(&dir).await
        } else {
            plaintext_keystore(&dir).await
        };
        let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
        let other_address = import_from_another_process(&path).await;
        let aliases_path = path.with_extension(ALIASES_FILE_EXTENSION);
        let changed_aliases = fs::read(&aliases_path).unwrap();

        let old_alias = keystore.aliases()[0].alias.clone();
        let error = keystore
            .update_alias(&old_alias, Some("renamed"))
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("changed after this command read it"),
            "{error}"
        );
        assert_eq!(fs::read(&aliases_path).unwrap(), changed_aliases);
        let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
        assert!(reloaded.get_alias(&other_address).is_ok());
    }
}

#[tokio::test]
async fn writes_fail_if_another_process_changed_only_the_aliases() {
    for encrypted in [false, true] {
        let dir = TempDir::new().unwrap();
        let (path, _) = if encrypted {
            encrypted_keystore(&dir).await
        } else {
            plaintext_keystore(&dir).await
        };
        let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
        // Renaming an alias in another process writes only the aliases file.
        let aliases = keystore.aliases();
        let (renamed_alias, other_alias) = (aliases[0].alias.clone(), aliases[1].alias.clone());
        FileBasedKeystore::load_or_create(&path)
            .unwrap()
            .update_alias(&renamed_alias, Some("renamed"))
            .await
            .unwrap();
        let aliases_path = path.with_extension(ALIASES_FILE_EXTENSION);
        let changed_aliases = fs::read(&aliases_path).unwrap();
        let keystore_contents = fs::read(&path).unwrap();

        if encrypted {
            keystore.unlock(PASSWORD).unwrap();
        }
        let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
        let error = keystore.import(None, key).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("changed after this command read it"),
            "{error}"
        );
        let error = keystore
            .update_alias(&other_alias, Some("other"))
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("changed after this command read it"),
            "{error}"
        );
        assert_eq!(fs::read(&aliases_path).unwrap(), changed_aliases);
        assert_eq!(fs::read(&path).unwrap(), keystore_contents);
        let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
        assert!(reloaded.alias_exists("renamed"));
    }
}

#[tokio::test]
async fn a_keystore_keeps_the_password_of_the_file_that_it_read() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();

    let mut other = FileBasedKeystore::load_or_create(&path).unwrap();
    other.unlock(PASSWORD).unwrap();
    other
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap();

    assert!(keystore.unlock("new password").is_err());
    keystore.unlock(PASSWORD).unwrap();
    let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let error = keystore.import(None, key).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed after this command read it"),
        "{error}"
    );
    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .unlock("new password")
        .unwrap();
}

#[tokio::test]
async fn plaintext_saves_do_not_undo_an_encryption_by_another_process() {
    let dir = TempDir::new().unwrap();
    let (path, _) = plaintext_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();

    let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let error = keystore.import(None, key).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed after this command read it"),
        "{error}"
    );
    assert!(
        FileBasedKeystore::load_or_create(&path)
            .unwrap()
            .is_encrypted()
    );
}

#[tokio::test]
async fn saving_a_locked_keystore_keeps_changes_from_other_processes() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let keystore = FileBasedKeystore::load_or_create(&path).unwrap();

    // Another process changes the password after this keystore was read.
    let mut other = FileBasedKeystore::load_or_create(&path).unwrap();
    other.unlock(PASSWORD).unwrap();
    other
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap();
    let changed_contents = fs::read(&path).unwrap();

    let error = keystore.save().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed after this command read it"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), changed_contents);
}

#[tokio::test]
async fn updates_fail_if_another_process_changed_the_keystore_after_unlocking() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = encrypted_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    // Another process adds a key after this keystore was unlocked, for example while a prompt
    // asked for a new password.
    let other_address = import_from_another_process(&path).await;
    let changed_contents = fs::read(&path).unwrap();

    let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let (unknown, _, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    let errors = [
        keystore.import(None, key).await.unwrap_err(),
        // An unlocked keystore rewrites its keys even when it removes an address that it does not
        // hold.
        keystore.remove(unknown).await.unwrap_err(),
        keystore
            .remove(*keys.keys().next().unwrap())
            .await
            .unwrap_err(),
        keystore
            .change_password_with_cost("new password", TEST_COST)
            .await
            .unwrap_err(),
        keystore.decrypt().await.unwrap_err(),
    ];
    for error in errors {
        assert!(
            error
                .to_string()
                .contains("changed after this command read it"),
            "{error}"
        );
    }
    assert_eq!(fs::read(&path).unwrap(), changed_contents);
    // The refused conversions left no recovery copy.
    assert!(!recovery_copy_path(&path).exists());
    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    reloaded.unlock(PASSWORD).unwrap();
    assert!(reloaded.addresses().contains(&other_address));

    // A password change by another process after this keystore was unlocked is detected too.
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    let mut other = FileBasedKeystore::load_or_create(&path).unwrap();
    other.unlock(PASSWORD).unwrap();
    other
        .change_password_with_cost("other password", TEST_COST)
        .await
        .unwrap();
    let error = keystore
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed after this command read it"),
        "{error}"
    );
    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .unlock("other password")
        .unwrap();
}

#[tokio::test]
async fn updates_after_unlocking_in_the_same_process_succeed() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = plaintext_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    for _ in 0..2 {
        let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
        keystore.import(None, key).await.unwrap();
    }
    keystore.remove(*keys.keys().next().unwrap()).await.unwrap();
    keystore
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap();
    let (_, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    keystore.import(None, key).await.unwrap();

    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    reloaded.unlock("new password").unwrap();
    assert_eq!(reloaded.key_pairs().unwrap().len(), keys.len() + 2);
}

#[cfg(unix)]
#[tokio::test]
async fn encrypts_a_keystore_in_a_read_only_directory() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let (path, keys) = plaintext_keystore(&dir).await;
    // The keystore file is written in place, so its directory does not need to be writable.
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await;
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    result.unwrap();

    let keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    assert!(keystore.is_encrypted());
    keystore.unlock(PASSWORD).unwrap();
    assert_eq!(keystore.key_pairs().unwrap().len(), keys.len());
}

#[cfg(unix)]
#[tokio::test]
async fn saves_after_a_failed_write_still_check_for_changes_by_other_processes() {
    use std::os::unix::fs::PermissionsExt;

    for another_process_writes in [false, true] {
        let dir = TempDir::new().unwrap();
        let (path, keys) = encrypted_keystore(&dir).await;
        let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
        keystore.unlock(PASSWORD).unwrap();

        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            // A privileged user can write the read-only file, so the write cannot fail.
            return;
        }
        let (address, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
        let error = keystore.import(None, key.copy()).await.unwrap_err();
        assert!(
            error.to_string().contains("Cannot write keystore to file"),
            "{error}"
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        if another_process_writes {
            // A password change in another process writes only the keystore file, so only the
            // check of that file can detect it.
            let mut other = FileBasedKeystore::load_or_create(&path).unwrap();
            other.unlock(PASSWORD).unwrap();
            other
                .change_password_with_cost("new password", TEST_COST)
                .await
                .unwrap();

            let error = keystore.import(None, key).await.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("changed after this command read it"),
                "{error}"
            );
            let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
            reloaded.unlock("new password").unwrap();
            assert!(!reloaded.addresses().contains(&address));
        } else {
            keystore.import(None, key).await.unwrap();
            let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
            reloaded.unlock(PASSWORD).unwrap();
            assert_eq!(reloaded.key_pairs().unwrap().len(), keys.len() + 1);
            assert!(reloaded.addresses().contains(&address));
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn only_the_owner_can_read_the_keystore_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let (path, _) = plaintext_keystore(&dir).await;
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[cfg(unix)]
#[tokio::test]
async fn encrypting_a_symlinked_keystore_keeps_the_symlink() {
    let dir = TempDir::new().unwrap();
    let (target, keys) = plaintext_keystore(&dir).await;
    let link = dir.path().join("link.keystore");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    FileBasedKeystore::load_or_create(&link)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let keystore = FileBasedKeystore::load_or_create(&target).unwrap();
    assert!(keystore.is_encrypted());
    keystore.unlock(PASSWORD).unwrap();
    assert_eq!(keystore.key_pairs().unwrap().len(), keys.len());
}

#[cfg(unix)]
#[tokio::test]
async fn encrypting_a_keystore_updates_all_of_its_hard_links() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = plaintext_keystore(&dir).await;
    let link = dir.path().join("link.keystore");
    fs::hard_link(&path, &link).unwrap();

    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    let contents = fs::read_to_string(&link).unwrap();
    for key in keys.values() {
        assert!(!contents.contains(&key.encode_base64()));
    }
    let keystore = FileBasedKeystore::load_or_create(&link).unwrap();
    assert!(keystore.is_encrypted());
    keystore.unlock(PASSWORD).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn saving_through_another_path_to_the_same_file_keeps_changes_from_other_processes() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let link = dir.path().join("link.keystore");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.set_path(&link);

    let mut other = FileBasedKeystore::load_or_create(&path).unwrap();
    other.unlock(PASSWORD).unwrap();
    other
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap();
    let changed_contents = fs::read(&path).unwrap();

    let error = keystore.save().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed after this command read it"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), changed_contents);
}

#[cfg(unix)]
#[tokio::test]
async fn writes_the_keystore_file_in_place() {
    use std::os::unix::fs::MetadataExt;

    // Writing in place keeps the inode, so that bind mounts, hard links, ownership, permissions
    // and extended attributes of the keystore file stay as they are.
    let dir = TempDir::new().unwrap();
    let (path, _) = plaintext_keystore(&dir).await;
    let inode = fs::metadata(&path).unwrap().ino();
    FileBasedKeystore::load_or_create(&path)
        .unwrap()
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
}

#[tokio::test]
async fn unlocking_an_unlocked_keystore_checks_the_password() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    assert!(keystore.unlock("not the password").is_err());
    keystore.unlock(PASSWORD).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn writing_through_a_dangling_symlink_creates_its_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target.keystore");
    let link = dir.path().join("sui.keystore");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let mut keystore = FileBasedKeystore::load_or_create(&link).unwrap();
    let (address, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    keystore.import(None, key).await.unwrap();
    keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let reloaded = FileBasedKeystore::load_or_create(&target).unwrap();
    assert!(reloaded.is_encrypted());
    reloaded.unlock(PASSWORD).unwrap();
    assert_eq!(reloaded.addresses(), vec![address]);
}

#[test]
fn concurrent_unlocks_derive_the_key_once() {
    let (_, file) = encrypt_test_keys();
    let state = UnlockState::default();
    let unlocks = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                state
                    .get_or_unlock(|| {
                        unlocks.fetch_add(1, Ordering::SeqCst);
                        // The unlock lasts long enough for the other threads to wait for it.
                        std::thread::sleep(Duration::from_millis(50));
                        Ok(decrypt_with_password(&file, PASSWORD)?.unwrap())
                    })
                    .unwrap();
            });
        }
    });
    assert_eq!(unlocks.load(Ordering::SeqCst), 1);
}

#[test]
fn a_failed_unlock_is_not_repeated() {
    let state = UnlockState::default();
    let unlocks = AtomicUsize::new(0);
    for _ in 0..3 {
        let error = state
            .get_or_unlock(|| {
                unlocks.fetch_add(1, Ordering::SeqCst);
                Err(anyhow!("The password is incorrect"))
            })
            .err()
            .unwrap();
        assert!(error.to_string().contains("The password is incorrect"));
    }
    assert_eq!(unlocks.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refused_changes_leave_the_keystore_as_its_files_hold_it() {
    for encrypted in [false, true] {
        let dir = TempDir::new().unwrap();
        let (path, keys) = if encrypted {
            encrypted_keystore(&dir).await
        } else {
            plaintext_keystore(&dir).await
        };
        let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
        keystore.unlock(PASSWORD).unwrap();
        let address = *keys.keys().next().unwrap();
        let alias = keystore.get_alias(&address).unwrap();
        import_from_another_process(&path).await;

        let (new_address, key, _, _) =
            generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
        assert!(
            keystore
                .import(Some("bot".to_string()), key.copy())
                .await
                .is_err()
        );
        assert!(!keystore.addresses().contains(&new_address));
        assert!(!keystore.alias_exists("bot"));

        assert!(keystore.remove(address).await.is_err());
        assert!(keystore.addresses().contains(&address));
        assert_eq!(keystore.get_alias(&address).unwrap(), alias);

        assert!(
            keystore
                .update_alias(&alias, Some("renamed"))
                .await
                .is_err()
        );
        assert_eq!(keystore.get_alias(&address).unwrap(), alias);

        // A keystore read again from its files accepts the change.
        let mut reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
        reloaded.unlock(PASSWORD).unwrap();
        reloaded.import(Some("bot".to_string()), key).await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_failed_write_of_the_keystore_file_does_not_write_the_aliases() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let (path, _) = plaintext_keystore(&dir).await;
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    let aliases_path = path.with_extension(ALIASES_FILE_EXTENSION);
    let aliases = fs::read(&aliases_path).unwrap();

    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    if fs::OpenOptions::new().write(true).open(&path).is_ok() {
        // A privileged user can write the read-only file, so the write cannot fail.
        return;
    }
    let (address, key, _, _) = generate_new_key(SignatureScheme::ED25519, None, None).unwrap();
    assert!(keystore.import(Some("bot".to_string()), key).await.is_err());
    assert_eq!(fs::read(&aliases_path).unwrap(), aliases);
    assert!(!keystore.addresses().contains(&address));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[tokio::test]
async fn conversions_remove_the_recovery_copy() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = plaintext_keystore(&dir).await;
    let copy_path = recovery_copy_path(&path);
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();

    keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    assert!(!copy_path.exists());
    keystore
        .change_password_with_cost("new password", TEST_COST)
        .await
        .unwrap();
    assert!(!copy_path.exists());
    keystore.decrypt().await.unwrap();
    assert!(!copy_path.exists());
    let reloaded = FileBasedKeystore::load_or_create(&path).unwrap();
    assert_eq!(reloaded.key_pairs().unwrap().len(), keys.len());
}

#[cfg(unix)]
#[tokio::test]
async fn a_conversion_that_leaves_the_keystore_file_unchanged_removes_the_recovery_copy() {
    use std::os::unix::fs::PermissionsExt;

    for encrypted in [false, true] {
        let dir = TempDir::new().unwrap();
        let (path, _) = if encrypted {
            encrypted_keystore(&dir).await
        } else {
            plaintext_keystore(&dir).await
        };
        let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
        keystore.unlock(PASSWORD).unwrap();
        let contents = fs::read(&path).unwrap();

        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            // A privileged user can write the read-only file, so the write cannot fail.
            return;
        }
        // Decrypting would otherwise leave the private keys in plaintext in the recovery copy.
        let result = if encrypted {
            keystore.decrypt().await
        } else {
            keystore.encrypt_with_cost(PASSWORD, TEST_COST).await
        };
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("Cannot write keystore to file"),
            "{error:#}"
        );
        assert_eq!(fs::read(&path).unwrap(), contents);
        assert!(!recovery_copy_path(&path).exists());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[tokio::test]
async fn a_keystore_file_that_cannot_be_read_names_its_recovery_copy() {
    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let copy_path = recovery_copy_path(&path);
    fs::copy(&path, &copy_path).unwrap();
    // A write that a crash interrupted after it truncated the keystore file.
    let contents = fs::read(&path).unwrap();
    fs::write(&path, &contents[..contents.len() / 2]).unwrap();

    let error = format!(
        "{:#}",
        FileBasedKeystore::load_or_create(&path).err().unwrap()
    );
    assert!(error.contains(&copy_path.display().to_string()), "{error}");
    fs::rename(&copy_path, &path).unwrap();
    let keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
}

/// The name of an environment variable that makes [`runs_in_a_new_process`] return `true`.
const NEW_PROCESS_ENV_VAR: &str = "SUI_KEYS_TEST_IN_NEW_PROCESS";

/// Runs the test `name` of this test binary again in a new process, with the environment variables
/// `vars`, and returns `false`. In that process it returns `true`, and the test runs there. Tests
/// that change the environment of their process run this way, so that the change does not reach
/// other tests.
#[cfg(unix)]
fn runs_in_a_new_process(name: &str, vars: &[(&str, &str)]) -> bool {
    if std::env::var_os(NEW_PROCESS_ENV_VAR).is_some() {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(NEW_PROCESS_ENV_VAR, "1")
        .env_remove(KEYSTORE_PASSWORD_ENV_VAR)
        .env_remove(NEW_KEYSTORE_PASSWORD_ENV_VAR)
        .envs(vars.iter().copied())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A name that matches no test would also succeed, without running anything.
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "The test {name} did not pass in a new process:\n{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

#[cfg(unix)]
#[test]
fn removed_password_variables_reach_the_keystore_but_not_child_processes() {
    if !runs_in_a_new_process(
        "encryption::encryption_tests::removed_password_variables_reach_the_keystore_but_not_child_processes",
        &[(KEYSTORE_PASSWORD_ENV_VAR, PASSWORD)],
    ) {
        return;
    }
    // SAFETY: this process runs only this test, and no other thread uses the environment.
    unsafe { remove_password_env_vars() };
    assert_eq!(
        password_from_env(KEYSTORE_PASSWORD_ENV_VAR)
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some(PASSWORD)
    );
    assert!(
        password_from_env(NEW_KEYSTORE_PASSWORD_ENV_VAR)
            .unwrap()
            .is_none()
    );
    assert!(std::env::var_os(KEYSTORE_PASSWORD_ENV_VAR).is_none());
    let child = std::process::Command::new("sh")
        .args(["-c", "printf %s \"${SUI_KEYSTORE_PASSWORD-unset}\""])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&child.stdout), "unset");
}

#[cfg(unix)]
#[tokio::test]
async fn recovery_copies_hold_only_encrypted_keystores() {
    let dir = TempDir::new().unwrap();
    let (path, keys) = encrypted_keystore(&dir).await;
    let copy_path = recovery_copy_path(&path);
    // A symlink at the path of the recovery copy keeps what a conversion wrote to the copy after
    // the conversion removes it.
    let written_copy = dir.path().join("written-copy");
    let encrypted_contents = fs::read(&path).unwrap();
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();

    std::os::unix::fs::symlink(&written_copy, &copy_path).unwrap();
    keystore.decrypt().await.unwrap();
    // The copy of a decryption holds the encrypted keystore as it was before.
    assert_eq!(fs::read(&written_copy).unwrap(), encrypted_contents);
    assert!(fs::symlink_metadata(&copy_path).is_err());

    std::os::unix::fs::symlink(&written_copy, &copy_path).unwrap();
    keystore
        .encrypt_with_cost(PASSWORD, TEST_COST)
        .await
        .unwrap();
    // The copy of an encryption holds the new encrypted keystore.
    assert_eq!(fs::read(&written_copy).unwrap(), fs::read(&path).unwrap());
    let written = String::from_utf8(fs::read(&written_copy).unwrap()).unwrap();
    for key in keys.values() {
        assert!(!written.contains(&key.encode_base64()));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_conversion_after_a_new_path_that_leaves_the_file_unchanged_removes_the_recovery_copy() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let (path, _) = encrypted_keystore(&dir).await;
    let other_path = dir.path().join("other.keystore");
    fs::copy(&path, &other_path).unwrap();
    let mut keystore = FileBasedKeystore::load_or_create(&path).unwrap();
    keystore.unlock(PASSWORD).unwrap();
    // The keystore did not read the file at its new path.
    keystore.set_path(&other_path);
    let contents = fs::read(&other_path).unwrap();

    fs::set_permissions(&other_path, fs::Permissions::from_mode(0o400)).unwrap();
    if fs::OpenOptions::new().write(true).open(&other_path).is_ok() {
        // A privileged user can write the read-only file, so the write cannot fail.
        return;
    }
    let error = format!("{:#}", keystore.decrypt().await.unwrap_err());
    assert!(error.contains("Cannot write keystore to file"), "{error}");
    assert!(!error.contains("can be damaged"), "{error}");
    assert!(!recovery_copy_path(&other_path).exists());
    assert_eq!(fs::read(&other_path).unwrap(), contents);
    fs::set_permissions(&other_path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn passwords_are_compared_in_their_unicode_nfc_form() {
    let keys = test_keys();
    // "é" as one code point, and as "e" followed by a combining acute accent.
    let composed = "caf\u{e9} au lait";
    let decomposed = "cafe\u{301} au lait";
    for (encrypted_with, decrypted_with) in [(composed, decomposed), (decomposed, composed)] {
        let file = KeystoreKey::generate(encrypted_with, TEST_COST)
            .unwrap()
            .encrypt(&keys)
            .unwrap();
        let unlocked = decrypt_with_password(&file, decrypted_with)
            .unwrap()
            .unwrap();
        assert_eq!(unlocked.keys, keys);
    }

    // NFC keeps compatibility characters, such as the ligature "ﬁ", apart from the letters that
    // they look like.
    let file = KeystoreKey::generate("\u{fb01}rst password", TEST_COST)
        .unwrap()
        .encrypt(&keys)
        .unwrap();
    assert!(
        decrypt_with_password(&file, "first password")
            .unwrap()
            .is_none()
    );
}
