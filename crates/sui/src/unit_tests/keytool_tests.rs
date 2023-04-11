// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::keytool::read_authority_keypair_from_file;
use crate::keytool::read_keypair_from_file;

use super::write_keypair_to_file;
use super::KeyToolCommand;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use rand::rngs::StdRng;
use rand::SeedableRng;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentScope;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, InMemKeystore, Keystore};
use sui_types::base_types::ObjectDigest;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::get_key_pair_from_rng;
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::Ed25519SuiSignature;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::Secp256k1SuiSignature;
use sui_types::crypto::Secp256r1SuiSignature;
use sui_types::crypto::Signature;
use sui_types::crypto::SignatureScheme;
use sui_types::crypto::SuiKeyPair;
use sui_types::crypto::SuiSignatureInner;
use sui_types::messages::TransactionData;
use sui_types::messages::TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
use tempfile::TempDir;

const TEST_MNEMONIC: &str = "result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss";

#[test]
fn test_addresses_command() -> Result<(), anyhow::Error> {
    // Add 3 Ed25519 KeyPairs as default
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(3));

    // Add another 3 Secp256k1 KeyPairs
    for _ in 0..3 {
        keystore.add_key(SuiKeyPair::Secp256k1(get_key_pair().1))?;
    }

    // List all addresses with flag
    KeyToolCommand::List.execute(&mut keystore).unwrap();
    Ok(())
}

#[test]
fn test_flag_in_signature_and_keypair() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));

    keystore.add_key(SuiKeyPair::Secp256k1(get_key_pair().1))?;
    keystore.add_key(SuiKeyPair::Ed25519(get_key_pair().1))?;

    for pk in keystore.keys() {
        let pk1 = pk.clone();
        let sig = keystore.sign_secure(&(&pk).into(), b"hello", Intent::sui_transaction())?;
        match sig {
            Signature::Ed25519SuiSignature(_) => {
                // signature contains corresponding flag
                assert_eq!(
                    *sig.as_ref().first().unwrap(),
                    Ed25519SuiSignature::SCHEME.flag()
                );
                // keystore stores pubkey with corresponding flag
                assert!(pk1.flag() == Ed25519SuiSignature::SCHEME.flag())
            }
            Signature::Secp256k1SuiSignature(_) => {
                assert_eq!(
                    *sig.as_ref().first().unwrap(),
                    Secp256k1SuiSignature::SCHEME.flag()
                );
                assert!(pk1.flag() == Secp256k1SuiSignature::SCHEME.flag())
            }
            Signature::Secp256r1SuiSignature(_) => {
                assert_eq!(
                    *sig.as_ref().first().unwrap(),
                    Secp256r1SuiSignature::SCHEME.flag()
                );
                assert!(pk1.flag() == Secp256r1SuiSignature::SCHEME.flag())
            }
        }
    }
    Ok(())
}

#[test]
fn test_read_write_keystore_with_flag() {
    let dir = tempfile::TempDir::new().unwrap();

    // create Secp256k1 keypair
    let kp_secp = SuiKeyPair::Secp256k1(get_key_pair().1);
    let addr_secp: SuiAddress = (&kp_secp.public()).into();
    let fp_secp = dir.path().join(format!("{}.key", addr_secp));
    let fp_secp_2 = fp_secp.clone();

    // write Secp256k1 keypair to file
    let res = write_keypair_to_file(&kp_secp, &fp_secp);
    assert!(res.is_ok());

    // read from file as enum KeyPair success
    let kp_secp_read = read_keypair_from_file(fp_secp);
    assert!(kp_secp_read.is_ok());

    // KeyPair wrote into file is the same as read
    assert_eq!(
        kp_secp_read.unwrap().public().as_ref(),
        kp_secp.public().as_ref()
    );

    // read as AuthorityKeyPair fails
    let kp_secp_read = read_authority_keypair_from_file(fp_secp_2);
    assert!(kp_secp_read.is_err());

    // create Ed25519 keypair
    let kp_ed = SuiKeyPair::Ed25519(get_key_pair().1);
    let addr_ed: SuiAddress = (&kp_ed.public()).into();
    let fp_ed = dir.path().join(format!("{}.key", addr_ed));
    let fp_ed_2 = fp_ed.clone();

    // write Ed25519 keypair to file
    let res = write_keypair_to_file(&kp_ed, &fp_ed);
    assert!(res.is_ok());

    // read from file as enum KeyPair success
    let kp_ed_read = read_keypair_from_file(fp_ed);
    assert!(kp_ed_read.is_ok());

    // KeyPair wrote into file is the same as read
    assert_eq!(
        kp_ed_read.unwrap().public().as_ref(),
        kp_ed.public().as_ref()
    );

    // read from file as AuthorityKeyPair success
    let kp_ed_read = read_authority_keypair_from_file(fp_ed_2);
    assert!(kp_ed_read.is_err());
}

#[test]
fn test_sui_operations_config() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("sui.keystore");
    let path1 = path.clone();
    // This is the hardcoded keystore in sui-operation: https://github.com/MystenLabs/sui-operations/blob/af04c9d3b61610dbb36401aff6bef29d06ef89f8/docker/config/generate/static/sui.keystore
    // If this test fails, address hardcoded in sui-operations is likely needed be updated.
    let kp = SuiKeyPair::decode_base64("ANRj4Rx5FZRehqwrctiLgZDPrY/3tI5+uJLCdaXPCj6C").unwrap();
    let contents = kp.encode_base64();
    let res = std::fs::write(path, contents);
    assert!(res.is_ok());
    let kp_read = read_keypair_from_file(path1);
    assert_eq!(
        SuiAddress::from_str("7d20dcdb2bca4f508ea9613994683eb4e76e9c4ed371169677c1be02aaf0b58e")
            .unwrap(),
        SuiAddress::from(&kp_read.unwrap().public())
    );

    // This is the hardcoded keystore in sui-operation: https://github.com/MystenLabs/sui-operations/blob/af04c9d3b61610dbb36401aff6bef29d06ef89f8/docker/config/generate/static/sui-benchmark.keystore
    // If this test fails, address hardcoded in sui-operations is likely needed be updated.
    let path2 = temp_dir.path().join("sui-benchmark.keystore");
    let path3 = path2.clone();
    let kp = SuiKeyPair::decode_base64("APCWxPNCbgGxOYKeMfPqPmXmwdNVyau9y4IsyBcmC14A").unwrap();
    let contents = kp.encode_base64();
    let res = std::fs::write(path2, contents);
    assert!(res.is_ok());
    let kp_read = read_keypair_from_file(path3);
    assert_eq!(
        SuiAddress::from_str("160ef6ce4f395208a12119c5011bf8d8ceb760e3159307c819bd0197d154d384")
            .unwrap(),
        SuiAddress::from(&kp_read.unwrap().public())
    );
}

#[test]
fn test_load_keystore_err() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("sui.keystore");
    let path2 = path.clone();

    // write encoded AuthorityKeyPair without flag byte to file
    let kp: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let contents = kp.encode_base64();
    let res = std::fs::write(path, contents);
    assert!(res.is_ok());

    // cannot load keypair due to missing flag
    assert!(FileBasedKeystore::new(&path2).is_err());
}

#[test]
fn test_mnemonics_ed25519() -> Result<(), anyhow::Error> {
    // Test case matches with /mysten/sui/sdk/typescript/test/unit/cryptography/ed25519-keypair.test.ts
    const TEST_CASES: [[&str; 3]; 3] = [["film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm", "AN0JMHpDum3BhrVwnkylH0/HGRHBQ/fO/8+MYOawO8j6", "a2d14fad60c56049ecf75246a481934691214ce413e6a8ae2fe6834c173a6133"],
    ["require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level", "AJrA997C1eVz6wYIp7bO8dpITSRBXpvg1m70/P3gusu2", "1ada6e6f3f3e4055096f606c746690f1108fcc2ca479055cc434a3e1d3f758aa"],
    ["organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake", "AAEMSIQeqyz09StSwuOW4MElQcZ+4jHW4/QcWlJEf5Yk", "e69e896ca10f5a77732769803cc2b5707f0ab9d4407afb5e4b4464b89769af14"]];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        KeyToolCommand::Import {
            mnemonic_phrase: t[0].to_string(),
            key_scheme: SignatureScheme::ED25519,
            derivation_path: None,
        }
        .execute(&mut keystore)?;
        let kp = SuiKeyPair::decode_base64(t[1]).unwrap();
        let addr = SuiAddress::from_str(t[2]).unwrap();
        assert_eq!(SuiAddress::from(&kp.public()), addr);
        assert!(keystore.addresses().contains(&addr));
    }
    Ok(())
}

#[test]
fn test_mnemonics_secp256k1() -> Result<(), anyhow::Error> {
    // Test case matches with /mysten/sui/sdk/typescript/test/unit/cryptography/secp256k1-keypair.test.ts
    const TEST_CASES: [[&str; 3]; 3] = [["film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm", "AQA9EYZoLXirIahsXHQMDfdi5DPQ72wLA79zke4EY6CP", "9e8f732575cc5386f8df3c784cd3ed1b53ce538da79926b2ad54dcc1197d2532"],
    ["require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level", "Ae+TTptXI6WaJfzplSrphnrbTD5qgftfMX5kTyca7unQ", "9fd5a804ed6b46d36949ff7434247f0fd594673973ece24aede6b86a7b5dae01"],
    ["organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake", "AY2iJpGSDMhvGILPjjpyeM1bV4Jky979nUenB5kvQeSj", "60287d7c38dee783c2ab1077216124011774be6b0764d62bd05f32c88979d5c5"]];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        KeyToolCommand::Import {
            mnemonic_phrase: t[0].to_string(),
            key_scheme: SignatureScheme::Secp256k1,
            derivation_path: None,
        }
        .execute(&mut keystore)?;
        let kp = SuiKeyPair::decode_base64(t[1]).unwrap();
        let addr = SuiAddress::from_str(t[2]).unwrap();
        assert_eq!(SuiAddress::from(&kp.public()), addr);
        assert!(keystore.addresses().contains(&addr));
    }
    Ok(())
}

#[test]
fn test_invalid_derivation_path() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/1'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_err());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/0'/784'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_err());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/54'/784'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_err());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/54'/784'/0'/0'/0'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_err());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/44'/784'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_err());

    Ok(())
}

#[test]
fn test_valid_derivation_path() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/784'/0'/0'/0'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_ok());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/784'/0'/0'/1'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_ok());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/784'/1'/0'/1'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_ok());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/54'/784'/0'/0/1".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_ok());

    assert!(KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/54'/784'/1'/0/1".parse().unwrap()),
    }
    .execute(&mut keystore)
    .is_ok());
    Ok(())
}

#[test]
fn test_keytool_bls12381() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
    KeyToolCommand::Generate {
        key_scheme: SignatureScheme::BLS12381,
        derivation_path: None,
        word_length: None,
    }
    .execute(&mut keystore)?;
    Ok(())
}

#[test]
fn test_sign_command() -> Result<(), anyhow::Error> {
    // Add a keypair
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(1));
    let binding = keystore.addresses();
    let sender = binding.first().unwrap();

    // Create a dummy TransactionData
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );
    let gas_price = 1;
    let tx_data = TransactionData::new_pay_sui(
        *sender,
        vec![gas],
        vec![SuiAddress::random_for_testing_only()],
        vec![10000],
        gas,
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        gas_price,
    )
    .unwrap();

    // Sign an intent message for the transaction data and a passed-in intent with scope as PersonalMessage.
    KeyToolCommand::Sign {
        address: *sender,
        data: Base64::encode(bcs::to_bytes(&tx_data)?),
        intent: Some(Intent::sui_app(IntentScope::PersonalMessage)),
    }
    .execute(&mut keystore)?;

    // Sign an intent message for the transaction data without intent passed in, so default is used.
    KeyToolCommand::Sign {
        address: *sender,
        data: Base64::encode(bcs::to_bytes(&tx_data)?),
        intent: None,
    }
    .execute(&mut keystore)?;
    Ok(())
}
