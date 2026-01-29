// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::nitro_attestation;

use sui::clock::{Self, Clock};

#[allow(unused_const)]
/// Error that the feature is not available on this network.
const ENotSupportedError: u64 = 0;
#[allow(unused_const)]
/// Error that the attestation input failed to be parsed.
const EParseError: u64 = 1;
#[allow(unused_const)]
/// Error that the attestation failed to be verified.
const EVerifyError: u64 = 2;
#[allow(unused_const)]
/// Error that the PCRs are invalid.
const EInvalidPCRsError: u64 = 3;

/// Represents a PCR entry with an index and value.
public struct PCREntry has drop {
    index: u8,
    value: vector<u8>,
}

/// Nitro Attestation Document defined for AWS.
public struct NitroAttestationDocument has drop {
    /// Issuing Nitro hypervisor module ID.
    module_id: vector<u8>,
    /// UTC time when document was created, in milliseconds since UNIX epoch.
    timestamp: u64,
    /// The digest function used for calculating the register values.
    digest: vector<u8>,
    /// A list of PCREntry containing the index and the PCR bytes.
    /// <https://docs.aws.amazon.com/enclaves/latest/user/set-up-attestation.html#where>.
    pcrs: vector<PCREntry>,
    /// An optional DER-encoded key the attestation, consumer can use to encrypt data with.
    public_key: Option<vector<u8>>,
    /// Additional signed user data, defined by protocol.
    user_data: Option<vector<u8>>,
    /// An optional cryptographic nonce provided by the attestation consumer as a proof of
    /// authenticity.
    nonce: Option<vector<u8>>,
}

/// @param attestation: attesttaion documents bytes data.
/// @param clock: the clock object.
///
/// Returns the parsed NitroAttestationDocument after verifying the attestation,
/// may abort with errors described above.
entry fun load_nitro_attestation(attestation: vector<u8>, clock: &Clock): NitroAttestationDocument {
    load_nitro_attestation_internal(&attestation, clock::timestamp_ms(clock))
}

public fun module_id(attestation: &NitroAttestationDocument): &vector<u8> {
    &attestation.module_id
}

public fun timestamp(attestation: &NitroAttestationDocument): &u64 {
    &attestation.timestamp
}

public fun digest(attestation: &NitroAttestationDocument): &vector<u8> {
    &attestation.digest
}

/// Returns a list of mapping PCREntry containg the index and the PCR bytes.
/// AWS supports PCR0-31. Required PCRs (index 0-4 & 8) are always included regardless of their 
/// value. Additional custom PCRs (index 5-7, 9-31) are also included if they are nonzeros.
public fun pcrs(attestation: &NitroAttestationDocument): &vector<PCREntry> {
    &attestation.pcrs
}

public fun public_key(attestation: &NitroAttestationDocument): &Option<vector<u8>> {
    &attestation.public_key
}

public fun user_data(attestation: &NitroAttestationDocument): &Option<vector<u8>> {
    &attestation.user_data
}

public fun nonce(attestation: &NitroAttestationDocument): &Option<vector<u8>> {
    &attestation.nonce
}

public fun index(entry: &PCREntry): u8 {
    entry.index
}

public fun value(entry: &PCREntry): &vector<u8> {
    &entry.value
}

/// Internal native function
native fun load_nitro_attestation_internal(
    attestation: &vector<u8>,
    current_timestamp: u64,
): NitroAttestationDocument;
