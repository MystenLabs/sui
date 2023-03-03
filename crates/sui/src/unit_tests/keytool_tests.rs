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
use sui_types::intent::Intent;
use sui_types::intent::IntentScope;
use sui_types::messages::TransactionData;
use tempfile::TempDir;

const TEST_MNEMONIC: &str = "result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss";

#[test]
fn test_addresses_command() -> Result<(), anyhow::Error> {
    // Add 3 Ed25519 KeyPairs as default
    let mut keystore = Keystore::from(InMemKeystore::new(3));

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
    let mut keystore = Keystore::from(InMemKeystore::new(0));

    keystore.add_key(SuiKeyPair::Secp256k1(get_key_pair().1))?;
    keystore.add_key(SuiKeyPair::Ed25519(get_key_pair().1))?;

    for pk in keystore.keys() {
        let pk1 = pk.clone();
        let sig = keystore.sign_secure(&(&pk).into(), b"hello", Intent::default())?;
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
        SuiAddress::from_str("849d63687330447431a2e76fecca4f3c10f6884ebaa9909674123c6c662612a3")
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
        SuiAddress::from_str("b9e0a0d97f83b5cd84a2df2f83c9d72af19a6952ed637a36dfacf884a2d6e27f")
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
    const TEST_CASES: [[&str; 3]; 3] = [["film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm", "AN0JMHpDum3BhrVwnkylH0/HGRHBQ/fO/8+MYOawO8j6", "8867068daf9111ee013450eea1b1e10ffd62fc874329f0eadadd62b5617011e4"],
    ["require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level", "AJrA997C1eVz6wYIp7bO8dpITSRBXpvg1m70/P3gusu2", "29bb131378438b6c7f50526e6a853a72ed97f10b75fc8127ca3faaf7e78add30"],
    ["organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake", "AAEMSIQeqyz09StSwuOW4MElQcZ+4jHW4/QcWlJEf5Yk", "6e5387db7249f6b0dc5b68eb095109157dc192a0392343d67688835fecdf14e4"]];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new(0));
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
    const TEST_CASES: [[&str; 3]; 3] = [["film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm", "AQA9EYZoLXirIahsXHQMDfdi5DPQ72wLA79zke4EY6CP", "88ea264013bd399f3cc69037aca2d6a0ce6adebb1accd679ab1cad80191894ca"],
    ["require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level", "Ae+TTptXI6WaJfzplSrphnrbTD5qgftfMX5kTyca7unQ", "fce7538e74e5df59529107d29f24991266e7165961932c205d85e04b00bc8a79"],
    ["organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake", "AY2iJpGSDMhvGILPjjpyeM1bV4Jky979nUenB5kvQeSj", "d6929233d383bc8fa8df95b69feb04d7c7b4fd154d9d59a2564e4d50d14a569d"]];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new(0));
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
    let mut keystore = Keystore::from(InMemKeystore::new(0));
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
    let mut keystore = Keystore::from(InMemKeystore::new(0));
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
    let mut keystore = Keystore::from(InMemKeystore::new(0));
    KeyToolCommand::Generate {
        key_scheme: SignatureScheme::BLS12381,
        derivation_path: None,
    }
    .execute(&mut keystore)?;
    Ok(())
}

#[test]
fn test_sign_command() -> Result<(), anyhow::Error> {
    // Add a keypair
    let mut keystore = Keystore::from(InMemKeystore::new(1));
    let binding = keystore.addresses();
    let sender = binding.first().unwrap();

    // Create a dummy TransactionData
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );
    let tx_data = TransactionData::new_pay_sui_with_dummy_gas_price(
        *sender,
        vec![gas],
        vec![SuiAddress::random_for_testing_only()],
        vec![10000],
        gas,
        1000,
    );

    // Sign an intent message for the transaction data and a passed-in intent with scope as PersonalMessage.
    KeyToolCommand::Sign {
        address: *sender,
        data: Base64::encode(bcs::to_bytes(&tx_data)?),
        intent: Some(Intent::default().with_scope(IntentScope::PersonalMessage)),
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
