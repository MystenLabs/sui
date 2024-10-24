// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::attestation;

/// @param attestation: attesttaion documents bytes data. 
/// @param enclave_pk: public key from enclave
///
/// If the signature is a valid Ed25519 signature of the message and public key, return true.
/// Otherwise, return false.
public native fun attestation_verify(
    enclave_pk: &vector<u8>,
    attestation: &vector<u8>,
): bool;

public native fun tpm2_attestation_verify(
    user_data: &vector<u8>,
    attestation: &vector<u8>,
): bool;
