// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Group operations of BLS12-381.
module sui::bulletproofs;

use sui::ristretto255::Point;
use sui::group_ops::{Self, Element};

public fun verify_range_proof(proof: &vector<u8>, range: u8, commitment: &Element<Point>): bool {
    verify_bulletproof_ristretto255(proof, range, commitment.bytes())
}

native fun verify_bulletproof_ristretto255(proof: &vector<u8>, range: u8, commitment: &vector<u8>): bool;
