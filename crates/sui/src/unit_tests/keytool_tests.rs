// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::keytool::read_authority_keypair_from_file;
use crate::keytool::read_keypair_from_file;

use super::write_keypair_to_file;
use super::KeyToolCommand;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, InMemKeystore, Keystore};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::get_key_pair_from_rng;
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::Ed25519SuiSignature;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::Secp256k1SuiSignature;
use sui_types::crypto::Signature;
use sui_types::crypto::SignatureScheme;
use sui_types::crypto::SuiKeyPair;
use sui_types::crypto::SuiSignatureInner;
use tempfile::TempDir;

const TEST_MNEMONIC: &str = "result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss";

#[test]
fn test_addresses_command() -> Result<(), anyhow::Error> {
    // Add 3 Ed25519 KeyPairs as default
    let mut keystore = Keystore::from(InMemKeystore::new(3));

    // Add another 3 Secp256k1 KeyPairs
    for _ in 0..3 {
        keystore.add_key(SuiKeyPair::Secp256k1SuiKeyPair(get_key_pair().1))?;
    }

    // List all addresses with flag
    KeyToolCommand::List.execute(&mut keystore).unwrap();
    Ok(())
}

#[test]
fn test_flag_in_signature_and_keypair() -> Result<(), anyhow::Error> {
    let mut keystore = Keystore::from(InMemKeystore::new(0));

    keystore.add_key(SuiKeyPair::Secp256k1SuiKeyPair(get_key_pair().1))?;
    keystore.add_key(SuiKeyPair::Ed25519SuiKeyPair(get_key_pair().1))?;

    for pk in keystore.keys() {
        let pk1 = pk.clone();
        let sig = keystore.sign(&(&pk).into(), b"hello")?;
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
        }
    }
    Ok(())
}

#[test]
fn test_read_write_keystore_with_flag() {
    let dir = tempfile::TempDir::new().unwrap();

    // create Secp256k1 keypair
    let kp_secp = SuiKeyPair::Secp256k1SuiKeyPair(get_key_pair().1);
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
    let kp_ed = SuiKeyPair::Ed25519SuiKeyPair(get_key_pair().1);
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
    let mut keystore = Keystore::from(InMemKeystore::new(0));
    KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::ED25519,
        derivation_path: None,
    }
    .execute(&mut keystore)?;
    keystore.keys().iter().for_each(|pk| {
        assert_eq!(
            Hex::encode(pk.as_ref()),
            "685b2d6f98784dd763249af21c92f588ca1be80c40a98c55bf7c91b74e5ac1e2"
        );
    });
    keystore.addresses().iter().for_each(|addr| {
        assert_eq!(
            addr.to_string(),
            "0x1a4623343cd42be47d67314fce0ad042f3c82685"
        );
    });
    Ok(())
}

#[test]
fn test_mnemonics_secp256k1() -> Result<(), anyhow::Error> {
    // Test case generated from https://microbitcoinorg.github.io/mnemonic/ with path m/54'/784'/0'/0/0
    let mut keystore = Keystore::from(InMemKeystore::new(0));
    KeyToolCommand::Import {
        mnemonic_phrase: TEST_MNEMONIC.to_string(),
        key_scheme: SignatureScheme::Secp256k1,
        derivation_path: None,
    }
    .execute(&mut keystore)?;
    keystore.keys().iter().for_each(|pk| {
        assert_eq!(
            Hex::encode(pk.as_ref()),
            "03e3717435582ab33d2e315d21e9bc4e19500a1fc4c8cdc73a15365891774b131f"
        );
    });
    keystore.addresses().iter().for_each(|addr| {
        assert_eq!(
            addr.to_string(),
            "0xed17b3f435c03ff69c2cdc6d394932e68375f20f"
        );
    });
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
