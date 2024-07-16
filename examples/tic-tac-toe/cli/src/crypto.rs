// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use fastcrypto::encoding::{Base64, Encoding};
use sui_types::{
    crypto::{EncodeDecodeBase64, PublicKey, SignatureScheme},
    multisig::MultiSigPublicKey,
};

/// Read a string as a Base64 encoded ED25519 public key.
pub(crate) fn public_key_from_base64(base64: &str) -> Result<PublicKey> {
    let bytes = Base64::decode(base64).map_err(|_| anyhow!("Failed to decode base64"))?;

    PublicKey::try_from_bytes(SignatureScheme::ED25519, &bytes)
        .map_err(|_| anyhow!("Failed to read public key"))
}

/// Combine public keys into a MultiSig. Keys are deduplicated before generation as multisigs cannot
/// contain the same public key twice.
pub(crate) fn combine_keys(keys: impl IntoIterator<Item = PublicKey>) -> Result<MultiSigPublicKey> {
    let dedupped: Vec<_> =
        BTreeMap::from_iter(keys.into_iter().map(|key| (key.encode_base64(), key)))
            .into_values()
            .collect();

    let weights = vec![1; dedupped.len()];
    Ok(MultiSigPublicKey::new(dedupped, weights, 1)?)
}
