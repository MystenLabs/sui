// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::{
    bls12381, ed25519,
    hash::{Blake2b256, HashFunction},
};
use shared_crypto::intent::INTENT_PREFIX_LENGTH;

// Here we select the types that are used by default in the code base.
// The whole code base should only:
// - refer to those aliases and not use the individual scheme implementations
// - not use the schemes in a way that break genericity (e.g. using their Struct impl functions)
// - swap one of those aliases to point to another type if necessary
//
// Beware: if you change those aliases to point to another scheme implementation, you will have
// to change all four aliases to point to concrete types that work with each other. Failure to do
// so will result in a ton of compilation errors, and worse: it will not make sense!

/// Network key signs network messages and blocks.
pub type NetworkPublicKey = ed25519::Ed25519PublicKey;
pub type NetworkPrivateKey = ed25519::Ed25519PrivateKey;
pub type NetworkKeyPair = ed25519::Ed25519KeyPair;
pub type NetworkKeySignature = ed25519::Ed25519Signature;
pub type NetworkKeySignatureAsBytes = ed25519::Ed25519SignatureAsBytes;

/// Protocol key is used in random beacon.
pub type ProtocolPublicKey = bls12381::min_sig::BLS12381PublicKey;
pub type ProtocolPublicKeyBytes = bls12381::min_sig::BLS12381PublicKeyAsBytes;
pub type ProtocolPrivateKey = bls12381::min_sig::BLS12381PrivateKey;
pub type ProtocolKeyPair = bls12381::min_sig::BLS12381KeyPair;

/// For block digest.
pub type DefaultHashFunction = Blake2b256;
pub const DIGEST_LENGTH: usize = DefaultHashFunction::OUTPUT_SIZE;
pub const INTENT_MESSAGE_LENGTH: usize = INTENT_PREFIX_LENGTH + DIGEST_LENGTH;
