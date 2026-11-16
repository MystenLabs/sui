// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::crypto::{
    AuthorityKeyPair, KeypairTraits, NetworkKeyPair, generate_proof_of_possession,
    get_key_pair_from_rng,
};
use fastcrypto::encoding::{Encoding, Hex};
use rand::{SeedableRng, rngs::StdRng};

// The uncompressed (serialized) encodings of the protocol key and proof of possession that
// `metadata()` produces. They were computed with `blst::min_sig::{PublicKey, Signature}::
// serialize`. `uncompressed_vectors_match_keys` checks that they decode to the same values.
const UNCOMPRESSED_PROTOCOL_PUBKEY: &str = "18d03a660fb1a0e0e73f889e853f6692083eaf74d71bec150894c6bbc74312c2f374c4306f8afb16cf4ea48da7052b211192dc943ce8b323ff3af92fdbfe4de91d9c4d2e9974449e757c560960ee0a6bc36ec01ad1332b492ab5d0440a8a3958175e680e3af9a81068b48eef7f7dcd798820aa388232d6033668e323672d6f196e344a24a42521e7ed6cb05797ca27e6099cdfa615872d39e43629d017c1fc38bc4118f270b0b080cc32a8f4707b8c42b7a1eefccf5ecf9218dc53c4fc03728f";
const UNCOMPRESSED_POP: &str = "0fbe4c5867bfcecfdfab4a176dc8490c278e0bf09fd0fda2203f59c51074a655567dc6586fa626cc3e6f3cbbe817b4e51359e4dd6190cfcb49438b6033445a07b6a859de30e9f96bfb9ce00833641b8b0aa6c8a2a1b6b2a361327f5a0d7b19a9";

fn uncompressed_pubkey() -> Vec<u8> {
    Hex::decode(UNCOMPRESSED_PROTOCOL_PUBKEY).unwrap()
}

fn uncompressed_pop() -> Vec<u8> {
    Hex::decode(UNCOMPRESSED_POP).unwrap()
}

fn metadata() -> ValidatorMetadataV1 {
    let sui_address = SuiAddress::from_bytes([3; 32]).unwrap();
    let (_, protocol_kp): (_, AuthorityKeyPair) =
        get_key_pair_from_rng(&mut StdRng::from_seed([7; 32]));
    let (_, network_kp): (_, NetworkKeyPair) =
        get_key_pair_from_rng(&mut StdRng::from_seed([8; 32]));
    let (_, worker_kp): (_, NetworkKeyPair) =
        get_key_pair_from_rng(&mut StdRng::from_seed([9; 32]));
    let pop = generate_proof_of_possession(&protocol_kp, sui_address);
    ValidatorMetadataV1 {
        sui_address,
        protocol_pubkey_bytes: protocol_kp.public().as_ref().to_vec(),
        network_pubkey_bytes: network_kp.public().as_ref().to_vec(),
        worker_pubkey_bytes: worker_kp.public().as_ref().to_vec(),
        proof_of_possession_bytes: pop.as_ref().to_vec(),
        name: "validator".to_string(),
        description: String::new(),
        image_url: String::new(),
        project_url: String::new(),
        net_address: "/dns/localhost/tcp/8080/http".to_string(),
        p2p_address: "/dns/localhost/udp/8081".to_string(),
        primary_address: "/dns/localhost/udp/8082".to_string(),
        worker_address: "/dns/localhost/udp/8083".to_string(),
        next_epoch_protocol_pubkey_bytes: None,
        next_epoch_proof_of_possession: None,
        next_epoch_network_pubkey_bytes: None,
        next_epoch_worker_pubkey_bytes: None,
        next_epoch_net_address: None,
        next_epoch_p2p_address: None,
        next_epoch_primary_address: None,
        next_epoch_worker_address: None,
        extra_fields: Bag::default(),
    }
}

fn with_next_epoch_key(
    mut metadata: ValidatorMetadataV1,
    pubkey: Vec<u8>,
    pop: Vec<u8>,
) -> ValidatorMetadataV1 {
    metadata.next_epoch_protocol_pubkey_bytes = Some(pubkey);
    metadata.next_epoch_proof_of_possession = Some(pop);
    metadata
}

fn verify_err(metadata: &ValidatorMetadataV1, require_compressed_bls: bool) -> u64 {
    metadata
        .verify_with_options(true, require_compressed_bls)
        .unwrap_err()
}

#[test]
fn uncompressed_vectors_match_keys() {
    let metadata = metadata();
    assert_eq!(uncompressed_pubkey().len(), 2 * AuthorityPublicKey::LENGTH);
    assert_eq!(uncompressed_pop().len(), 2 * AuthoritySignature::LENGTH);
    assert_eq!(
        AuthorityPublicKey::from_bytes(&uncompressed_pubkey()).unwrap(),
        AuthorityPublicKey::from_bytes(&metadata.protocol_pubkey_bytes).unwrap(),
    );
    assert_eq!(
        AuthoritySignature::from_bytes(&uncompressed_pop()).unwrap(),
        AuthoritySignature::from_bytes(&metadata.proof_of_possession_bytes).unwrap(),
    );
}

#[test]
fn compressed_protocol_key_is_accepted() {
    let metadata = metadata();
    assert_eq!(
        metadata.protocol_pubkey_bytes.len(),
        AuthorityPublicKey::LENGTH
    );
    assert_eq!(
        metadata.proof_of_possession_bytes.len(),
        AuthoritySignature::LENGTH
    );
    for require_compressed_bls in [false, true] {
        metadata
            .verify_with_options(true, require_compressed_bls)
            .unwrap();
    }

    let next_epoch = with_next_epoch_key(
        metadata.clone(),
        metadata.protocol_pubkey_bytes.clone(),
        metadata.proof_of_possession_bytes.clone(),
    );
    for require_compressed_bls in [false, true] {
        next_epoch
            .verify_with_options(true, require_compressed_bls)
            .unwrap();
    }
}

#[test]
fn uncompressed_protocol_key_is_rejected() {
    let mut current = metadata();
    current.protocol_pubkey_bytes = uncompressed_pubkey();
    // The legacy rules accept the uncompressed encoding.
    current.verify_with_options(true, false).unwrap();
    assert_eq!(verify_err(&current, true), E_METADATA_INVALID_PUBKEY);

    let metadata = metadata();
    let next_epoch = with_next_epoch_key(
        metadata.clone(),
        uncompressed_pubkey(),
        metadata.proof_of_possession_bytes.clone(),
    );
    next_epoch.verify_with_options(true, false).unwrap();
    assert_eq!(verify_err(&next_epoch, true), E_METADATA_INVALID_PUBKEY);
}

#[test]
fn uncompressed_proof_of_possession_is_rejected() {
    let mut current = metadata();
    current.proof_of_possession_bytes = uncompressed_pop();
    current.verify_with_options(true, false).unwrap();
    assert_eq!(verify_err(&current, true), E_METADATA_INVALID_POP);

    let metadata = metadata();
    let next_epoch = with_next_epoch_key(
        metadata.clone(),
        metadata.protocol_pubkey_bytes.clone(),
        uncompressed_pop(),
    );
    next_epoch.verify_with_options(true, false).unwrap();
    assert_eq!(verify_err(&next_epoch, true), E_METADATA_INVALID_POP);
}

#[test]
fn wrong_length_protocol_key_is_rejected() {
    let metadata = metadata();
    let compressed = metadata.protocol_pubkey_bytes.clone();
    let uncompressed = uncompressed_pubkey();
    let wrong_lengths = [
        vec![],
        compressed[..AuthorityPublicKey::LENGTH - 1].to_vec(),
        [compressed.as_slice(), &[0]].concat(),
        uncompressed[..2 * AuthorityPublicKey::LENGTH - 1].to_vec(),
        [uncompressed.as_slice(), &[0]].concat(),
    ];
    for bytes in wrong_lengths {
        let mut current = metadata.clone();
        current.protocol_pubkey_bytes = bytes.clone();
        assert_eq!(verify_err(&current, true), E_METADATA_INVALID_PUBKEY);

        let next_epoch = with_next_epoch_key(
            metadata.clone(),
            bytes,
            metadata.proof_of_possession_bytes.clone(),
        );
        assert_eq!(verify_err(&next_epoch, true), E_METADATA_INVALID_PUBKEY);
    }
}

#[test]
fn wrong_length_proof_of_possession_is_rejected() {
    let metadata = metadata();
    let compressed = metadata.proof_of_possession_bytes.clone();
    let uncompressed = uncompressed_pop();
    let wrong_lengths = [
        vec![],
        compressed[..AuthoritySignature::LENGTH - 1].to_vec(),
        [compressed.as_slice(), &[0]].concat(),
        uncompressed[..2 * AuthoritySignature::LENGTH - 1].to_vec(),
        [uncompressed.as_slice(), &[0]].concat(),
    ];
    for bytes in wrong_lengths {
        let mut current = metadata.clone();
        current.proof_of_possession_bytes = bytes.clone();
        assert_eq!(verify_err(&current, true), E_METADATA_INVALID_POP);

        let next_epoch = with_next_epoch_key(
            metadata.clone(),
            metadata.protocol_pubkey_bytes.clone(),
            bytes,
        );
        assert_eq!(verify_err(&next_epoch, true), E_METADATA_INVALID_POP);
    }
}

#[test]
fn verify_keeps_legacy_encoding_rules() {
    let mut metadata = metadata();
    metadata.protocol_pubkey_bytes = uncompressed_pubkey();
    metadata.proof_of_possession_bytes = uncompressed_pop();
    metadata.verify(true).unwrap();
    metadata.verify(false).unwrap();
}
