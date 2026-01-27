// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Group operations of BLS12-381.
module sui::bulletproofs;

native fun verify_bulletproof_ristretto255(proof: &vector<u8>, range: u8, commitment: &vector<u8>): bool;