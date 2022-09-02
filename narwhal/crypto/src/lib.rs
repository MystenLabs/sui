// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use fastcrypto::ed25519;

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

pub type PublicKey = ed25519::Ed25519PublicKey;
pub type Signature = ed25519::Ed25519Signature;
pub type AggregateSignature = ed25519::Ed25519AggregateSignature;
pub type PrivateKey = ed25519::Ed25519PrivateKey;
pub type KeyPair = ed25519::Ed25519KeyPair;

// Example to use BLS12-377 instead:
// #[cfg(feature = "celo")]
// pub type PublicKey = bls12377::BLS12377PublicKey;
// #[cfg(feature = "celo")]
// pub type Signature = bls12377::BLS12377Signature;
// #[cfg(feature = "celo")]
// pub type PrivateKey = bls12377::BLS12377PrivateKey;
// #[cfg(feature = "celo")]
// pub type KeyPair = bls12377::BLS12377KeyPair;
////////////////////////////////////////////////////////////////////////

#[cfg(all(test, feature = "celo"))]
#[path = "tests/bls12377_tests.rs"]
pub mod bls12377_tests;

#[cfg(feature = "celo")]
pub mod bls12377;
