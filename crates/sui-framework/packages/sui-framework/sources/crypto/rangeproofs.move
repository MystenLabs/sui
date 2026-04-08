// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::rangeproofs;

use sui::group_ops::Element;
use sui::ristretto255;

#[allow(unused_const)]
const ENotSupported: u64 = 0; // Operation is not supported by the network.
#[allow(unused_const)]
const EInvalidProof: u64 = 1;
#[allow(unused_const)]
const EInvalidRange: u64 = 2;
#[allow(unused_const)]
const EInvalidBatchSize: u64 = 3;
const EUnsupportedVersion: u64 = 4;

/// Verify a range proof over the Ristretto255 curve that all committed values are in the range [0, 2^bits).
/// Currently, the only supported version is 0 which corresponds to the original Bulletproofs construction (https://eprint.iacr.org/2017/1066.pdf).
/// In the future, we may add support for newer versions of Bulletproofs, such as Bulletproofs+ or Bulletproofs++.
///
/// The format of the proof follows the specifications from https://github.com/dalek-cryptography/bulletproofs/blob/be67b6d5f5ad1c1f54d5511b52e6d645a1313d07/src/range_proof/mod.rs#L59-L76.
///
/// The `bits` parameter is the bit length of the range and must be one of 8, 16, 32, or 64.
///
/// The `commitments` are Pedersen commitments to the values used in the proof.
/// The number of commitments must be a power of two, but if needed, the input to the prover can be padded with trivial commitments to zero.
/// The number of commitments times `bits` can be at most 512.
///
/// Enabled only on devnet.
public fun verify_bulletproofs_ristretto255(
    proof: &vector<u8>,
    bits: u8,
    commitments: &vector<Element<ristretto255::G>>,
    version: u8,
): bool {
    match (version) {
        0 => verify_bulletproofs_ristretto255_internal(
            proof,
            bits,
            &commitments.map_ref!(|c| *c.bytes()),
        ),
        _ => abort EUnsupportedVersion,
    }
}

native fun verify_bulletproofs_ristretto255_internal(
    proof: &vector<u8>,
    bits: u8,
    commitments: &vector<vector<u8>>,
): bool;
