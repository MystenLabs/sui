// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::keytool::read_authority_keypair_from_file;
use crate::keytool::read_keypair_from_file;

use super::write_keypair_to_file;
use super::KeyToolCommand;
use anyhow::Ok;
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
use sui_types::transaction::TransactionData;
use sui_types::transaction::TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
use tempfile::TempDir;
use tokio::test;

const TEST_MNEMONIC: &str = "result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss";

#[test]
async fn test_addresses_command() -> Result<(), anyhow::Error> {
    // Add 3 Ed25519 KeyPairs as default
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(3));

    // Add another 3 Secp256k1 KeyPairs
    for _ in 0..3 {
        keystore.add_key(SuiKeyPair::Secp256k1(get_key_pair().1))?;
    }

    // List all addresses with flag
    KeyToolCommand::List.execute(&mut keystore).await.unwrap();
    Ok(())
}

#[test]
async fn test_flag_in_signature_and_keypair() -> Result<(), anyhow::Error> {
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
async fn test_read_write_keystore_with_flag() {
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
async fn test_sui_operations_config() {
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
async fn test_load_keystore_err() {
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
async fn test_private_keys_ed25519() -> Result<(), anyhow::Error> {
    // private key, base64, address
    const TEST_CASES: &[(&str, &str, &str)] = &[
        (
            "0x9dd9ae36ee51b912a0364c58c1f21333bcdad2d91911aa127226c512be285102",
            "AJ3ZrjbuUbkSoDZMWMHyEzO82tLZGRGqEnImxRK+KFEC",
            "0x90f3e6d73b5730f16974f4df1d3441394ebae62186baf83608599f226455afa7",
        ),
        (
            "0xeea84be738c59f56ee94dae8fd5a68082d4579ed38548d6ec4017da6c5619bf3",
            "AO6oS+c4xZ9W7pTa6P1aaAgtRXntOFSNbsQBfabFYZvz",
            "0xfd233cd9a5dd7e577f16fa523427c75fbc382af1583c39fdf1c6747d2ed807a3",
        ),
        (
            "0x91e8808c489ee0cc99c2edf79e63be6a144f8f600c7411bc2e806f7255710674",
            "AJHogIxInuDMmcLt955jvmoUT49gDHQRvC6Ab3JVcQZ0",
            "0x81aaefa4a883e72e8b6ccd3bec307e25fe3d79b14e43b778695c55dcec42f4f0",
        ),
    ];
    // assert correctness
    for (private_key, base64, address) in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        KeyToolCommand::Import {
            input_string: private_key.to_string(),
            key_scheme: SignatureScheme::ED25519,
            derivation_path: None,
        }
        .execute(&mut keystore)
        .await?;
        let kp = SuiKeyPair::decode_base64(base64).unwrap();
        let addr = SuiAddress::from_str(address).unwrap();
        assert_eq!(SuiAddress::from(&kp.public()), addr);
        assert!(keystore.addresses().contains(&addr));
    }

    // assert failure when private key is malformed
    for (private_key, _, _) in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        let output = KeyToolCommand::Import {
            input_string: private_key[..25].to_string(),
            key_scheme: SignatureScheme::ED25519,
            derivation_path: None,
        }
        .execute(&mut keystore)
        .await;
        assert!(output.is_err());
    }

    Ok(())
}

#[test]
async fn test_mnemonics_ed25519() -> Result<(), anyhow::Error> {
    // Test case matches with /mysten/sui/sdk/typescript/test/unit/cryptography/ed25519-keypair.test.ts
    const TEST_CASES: [[&str; 3]; 3] = [["film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm", "AN0JMHpDum3BhrVwnkylH0/HGRHBQ/fO/8+MYOawO8j6", "a2d14fad60c56049ecf75246a481934691214ce413e6a8ae2fe6834c173a6133"],
    ["require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level", "AJrA997C1eVz6wYIp7bO8dpITSRBXpvg1m70/P3gusu2", "1ada6e6f3f3e4055096f606c746690f1108fcc2ca479055cc434a3e1d3f758aa"],
    ["organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake", "AAEMSIQeqyz09StSwuOW4MElQcZ+4jHW4/QcWlJEf5Yk", "e69e896ca10f5a77732769803cc2b5707f0ab9d4407afb5e4b4464b89769af14"]];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        KeyToolCommand::Import {
            input_string: t[0].to_string(),
            key_scheme: SignatureScheme::ED25519,
            derivation_path: None,
        }
        .execute(&mut keystore)
        .await?;
        let kp = SuiKeyPair::decode_base64(t[1]).unwrap();
        let addr = SuiAddress::from_str(t[2]).unwrap();
        assert_eq!(SuiAddress::from(&kp.public()), addr);
        assert!(keystore.addresses().contains(&addr));
    }
    Ok(())
}

#[test]
async fn test_mnemonics_secp256k1() -> Result<(), anyhow::Error> {
    // Test case matches with /mysten/sui/sdk/typescript/test/unit/cryptography/secp256k1-keypair.test.ts
    const TEST_CASES: [[&str; 3]; 3] = [["film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm", "AQA9EYZoLXirIahsXHQMDfdi5DPQ72wLA79zke4EY6CP", "9e8f732575cc5386f8df3c784cd3ed1b53ce538da79926b2ad54dcc1197d2532"],
    ["require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level", "Ae+TTptXI6WaJfzplSrphnrbTD5qgftfMX5kTyca7unQ", "9fd5a804ed6b46d36949ff7434247f0fd594673973ece24aede6b86a7b5dae01"],
    ["organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake", "AY2iJpGSDMhvGILPjjpyeM1bV4Jky979nUenB5kvQeSj", "60287d7c38dee783c2ab1077216124011774be6b0764d62bd05f32c88979d5c5"]];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        KeyToolCommand::Import {
            input_string: t[0].to_string(),
            key_scheme: SignatureScheme::Secp256k1,
            derivation_path: None,
        }
        .execute(&mut keystore)
        .await?;
        let kp = SuiKeyPair::decode_base64(t[1]).unwrap();
        let addr = SuiAddress::from_str(t[2]).unwrap();
        assert_eq!(SuiAddress::from(&kp.public()), addr);
        assert!(keystore.addresses().contains(&addr));
    }
    Ok(())
}

#[test]
async fn test_mnemonics_secp256r1() -> Result<(), anyhow::Error> {
    // Test case matches with /mysten/sui/sdk/typescript/test/unit/cryptography/secp256r1-keypair.test.ts
    const TEST_CASES: [[&str; 3]; 3] = [
        [
            "act wing dilemma glory episode region allow mad tourist humble muffin oblige",
            "AiWmZXUcFpUF75H082F2RVJAABS5kcrvb8o09IPH9yUw",
            "0x4a822457f1970468d38dae8e63fb60eefdaa497d74d781f581ea2d137ec36f3a",
        ],
        [
            "flag rebel cabbage captain minimum purpose long already valley horn enrich salt",
            "AjaB6aLp4fQabx4NglfGz2Bf01TGKArV80NEOnqDwqNN",
            "0xcd43ecb9dd32249ff5748f5e4d51855b01c9b1b8bbe7f8638bb8ab4cb463b920",
        ],
        [
            "area renew bar language pudding trial small host remind supreme cabbage era",
            "AtSIEzVpJv+bJH3XptEq63vsuK+te1KRSY7JsiuJfcdK",
            "0x0d9047b7e7b698cc09c955ea97b0c68c2be7fb3aebeb59edcc84b1fb87e0f28e",
        ],
    ];

    for t in TEST_CASES {
        let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
        KeyToolCommand::Import {
            input_string: t[0].to_string(),
            key_scheme: SignatureScheme::Secp256r1,
            derivation_path: None,
        }
        .execute(&mut keystore)
        .await?;

        let kp = SuiKeyPair::decode_base64(t[1]).unwrap();
        println!("{:?}", kp.public().encode_base64());
        let addr = SuiAddress::from_str(t[2]).unwrap();
        assert_eq!(SuiAddress::from(&kp.public()), addr);
        assert!(keystore.addresses().contains(&addr));
    }

    Ok(())
}

#[test]
async fn test_invalid_derivation_path() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/1'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_err());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/0'/784'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_err());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/54'/784'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_err());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/54'/784'/0'/0'/0'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_err());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/44'/784'/0'/0/0".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_err());

    Ok(())
}

#[test]
async fn test_valid_derivation_path() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/784'/0'/0'/0'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_ok());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/784'/0'/0'/1'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_ok());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: Some("m/44'/784'/1'/0'/1'".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_ok());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/54'/784'/0'/0/1".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_ok());

    assert!(KeyToolCommand::Import {
        input_string: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: Some("m/54'/784'/1'/0/1".parse().unwrap()),
    }
    .execute(&mut keystore)
    .await
    .is_ok());
    Ok(())
}

#[test]
async fn test_keytool_bls12381() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new_insecure_for_tests(0));
    KeyToolCommand::Generate {
        key_scheme: SignatureScheme::BLS12381,
        derivation_path: None,
        word_length: None,
    }
    .execute(&mut keystore)
    .await?;
    Ok(())
}

#[test]
async fn test_sign_command() -> Result<(), anyhow::Error> {
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
    .execute(&mut keystore)
    .await?;

    // Sign an intent message for the transaction data without intent passed in, so default is used.
    KeyToolCommand::Sign {
        address: *sender,
        data: Base64::encode(bcs::to_bytes(&tx_data)?),
        intent: None,
    }
    .execute(&mut keystore)
    .await?;
    Ok(())
}
