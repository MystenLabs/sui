// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::anyhow;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::{secp256k1::Secp256k1KeyPair, traits::EncodeDecodeBase64};
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair, SuiKeyPair, ToFromBytes};

/// Write Base64 encoded `flag || privkey` to file.
pub fn write_keypair_to_file<P: AsRef<std::path::Path>>(
    keypair: &SuiKeyPair,
    path: P,
) -> anyhow::Result<()> {
    let contents = keypair.encode_base64();
    std::fs::write(path, contents)?;
    Ok(())
}

/// Write Base64 encoded `privkey` to file.
pub fn write_authority_keypair_to_file<P: AsRef<std::path::Path>>(
    keypair: &AuthorityKeyPair,
    path: P,
) -> anyhow::Result<()> {
    let contents = keypair.encode_base64();
    std::fs::write(path, contents)?;
    Ok(())
}

/// Read from file as Base64 encoded `privkey` and return a AuthorityKeyPair.
pub fn read_authority_keypair_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<AuthorityKeyPair> {
    let contents = std::fs::read_to_string(path)?;
    AuthorityKeyPair::decode_base64(contents.as_str().trim()).map_err(|e| anyhow!(e))
}

/// Read from file as Base64 encoded `flag || privkey` and return a SuiKeypair.
pub fn read_keypair_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<SuiKeyPair> {
    let contents = std::fs::read_to_string(path)?;
    SuiKeyPair::decode_base64(contents.as_str().trim()).map_err(|e| anyhow!(e))
}

/// Read from file as Base64 encoded `flag || privkey` and return a NetworkKeyPair.
pub fn read_network_keypair_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<NetworkKeyPair> {
    let kp = read_keypair_from_file(path)?;
    if let SuiKeyPair::Ed25519(kp) = kp {
        Ok(kp)
    } else {
        Err(anyhow!("Invalid scheme for network keypair"))
    }
}

/// Read a SuiKeyPair from a file. The content could be any of the following:
/// - Base64 encoded `flag || privkey` for ECDSA key
/// - Base64 encoded `privkey` for Raw key
/// - Bech32 encoded private key prefixed with `suiprivkey`
/// - Hex encoded `privkey` for Raw key
///
/// If `require_secp256k1` is true, it will return an error if the key is not Secp256k1.
pub fn read_key(path: &PathBuf, require_secp256k1: bool) -> Result<SuiKeyPair, anyhow::Error> {
    if !path.exists() {
        return Err(anyhow::anyhow!("Key file not found at path: {:?}", path));
    }
    let file_contents = std::fs::read_to_string(path)?;
    let contents = file_contents.as_str().trim();

    // Try base64 encoded SuiKeyPair `flag || privkey`
    if let Ok(key) = SuiKeyPair::decode_base64(contents) {
        if require_secp256k1 && !matches!(key, SuiKeyPair::Secp256k1(_)) {
            return Err(anyhow!("Key is not Secp256k1"));
        }
        return Ok(key);
    }

    // Try base64 encoded Raw Secp256k1 key `privkey`
    if let Ok(key) = Secp256k1KeyPair::decode_base64(contents) {
        return Ok(SuiKeyPair::Secp256k1(key));
    }

    // Try Bech32 encoded 33-byte `flag || private key` starting with `suiprivkey`A prefix.
    // This is the format of a private key exported from Sui Wallet or sui.keystore.
    if let Ok(key) = SuiKeyPair::decode(contents) {
        if require_secp256k1 && !matches!(key, SuiKeyPair::Secp256k1(_)) {
            return Err(anyhow!("Key is not Secp256k1"));
        }
        return Ok(key);
    }

    // Try hex encoded Raw key `privkey`
    if let Ok(bytes) = Hex::decode(contents).map_err(|e| anyhow!("Error decoding hex: {:?}", e)) {
        if let Ok(key) = Secp256k1KeyPair::from_bytes(&bytes) {
            return Ok(SuiKeyPair::Secp256k1(key));
        }
    }

    Err(anyhow!("Error decoding key from {:?}", path))
}
