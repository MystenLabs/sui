// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::nitro_attestation;

use sui::clock::{Self, Clock};

/// Internal native function
native fun verify_nitro_attestation_internal(
    attestation: &vector<u8>,
    current_timestamp: u64
): vector<vector<u8>>;

/// @param attestation: attesttaion documents bytes data. 
/// @param clock: the clock object.
///
/// Returns parsed pcrs after verifying the attestation.
public fun verify_nitro_attestation(
    attestation: &vector<u8>,
    clock: &Clock
): vector<vector<u8>> {
    verify_nitro_attestation_internal(attestation, clock::timestamp_ms(clock))
}