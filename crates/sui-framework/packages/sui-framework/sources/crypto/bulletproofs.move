// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::bulletproofs;

use sui::group_ops::Element;
use sui::ristretto255;

#[allow(unused_const)]
const ENotSupported: u64 = 0; // Operation is not supported by the network.
const EInvalidProof: u64 = 1;
#[allow(unused_const)]
const EInvalidCommitment: u64 = 2;
const EInvalidRange: u64 = 3;
const EInvalidBatchSize: u64 = 4;
const EUnsupportedVersion: u64 = 5;

/// Verify a range proof over the Ristretto255 curve that all committed values are in the range [0, 2^bits).
/// Currently, the only supported version is 0 which corresponds to the original Bulletproofs construction (https://eprint.iacr.org/2017/1066.pdf).
/// In the future, we may add support for newer versions of Bulletproofs, such as Bulletproofs+ or Bulletproofs++.
public fun verify_range_proof_ristretto255(proof: &vector<u8>, bits: u8, commitments: &vector<Element<ristretto255::G>>, version: u8): bool {
    match (version) {
        0 => verify_bulletproof_ristretto255_internal(proof, bits, &commitments.map_ref!(|c| *c.bytes())),
        _ => abort EUnsupportedVersion,
    }
}

native fun verify_bulletproof_ristretto255_internal(
    proof: &vector<u8>,
    bits: u8,
    commitments: &vector<vector<u8>>,
): bool;
