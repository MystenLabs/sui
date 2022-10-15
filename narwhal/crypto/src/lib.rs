// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use fastcrypto::{
    bls12381, ed25519,
    hash::{Blake2b256, HashFunction},
};

// This re-export allows using the trait-defined APIs
pub use fastcrypto::traits;

////////////////////////////////////////////////////////////////////////
/// Type aliases selecting the signature algorithm for the code base.
////////////////////////////////////////////////////////////////////////
// Here we select the types that are used by default in the code base.
// The whole code base should only:
// - refer to those aliases and not use the individual scheme implementations
// - not use the schemes in a way that break genericity (e.g. using their Struct impl functions)
// - swap one of those aliases to point to another type if necessary
//
// Beware: if you change those aliases to point to another scheme implementation, you will have
// to change all four aliases to point to concrete types that work with each other. Failure to do
// so will result in a ton of compilation errors, and worse: it will not make sense!

pub type PublicKey = bls12381::BLS12381PublicKey;
pub type Signature = bls12381::BLS12381Signature;
pub type AggregateSignature = bls12381::BLS12381AggregateSignature;
pub type PrivateKey = bls12381::BLS12381PrivateKey;
pub type KeyPair = bls12381::BLS12381KeyPair;

// Example to use BLS12-377 instead:
// #[cfg(feature = "celo")]
// pub type PublicKey = bls12377::BLS12377PublicKey;
// #[cfg(feature = "celo")]
// pub type Signature = bls12377::BLS12377Signature;
// #[cfg(feature = "celo")]
// pub type PrivateKey = bls12377::BLS12377PrivateKey;
// #[cfg(feature = "celo")]
// pub type KeyPair = bls12377::BLS12377KeyPair;

pub type NetworkPublicKey = ed25519::Ed25519PublicKey;
pub type NetworkKeyPair = ed25519::Ed25519KeyPair;

////////////////////////////////////////////////////////////////////////

// Type alias selecting the default hash function for the code base.
pub type DefaultHashFunction = Blake2b256;
pub const DIGEST_LENGTH: usize = DefaultHashFunction::OUTPUT_SIZE;

#[cfg(all(test, feature = "celo"))]
#[path = "tests/bls12377_tests.rs"]
pub mod bls12377_tests;

#[cfg(feature = "celo")]
pub mod bls12377;
