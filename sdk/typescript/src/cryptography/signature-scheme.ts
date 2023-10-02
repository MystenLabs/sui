// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export const SIGNATURE_SCHEME_TO_FLAG = {
	ED25519: 0x00,
	Secp256k1: 0x01,
	Secp256r1: 0x02,
	MultiSig: 0x03,
	ZkLogin: 0x05,
} as const;

export const SIGNATURE_SCHEME_TO_SIZE = {
	ED25519: 32,
	Secp256k1: 33,
	Secp256r1: 33,
};

export const SIGNATURE_FLAG_TO_SCHEME = {
	0x00: 'ED25519',
	0x01: 'Secp256k1',
	0x02: 'Secp256r1',
	0x03: 'MultiSig',
	0x05: 'ZkLogin',
} as const;

export type SignatureScheme = 'ED25519' | 'Secp256k1' | 'Secp256r1' | 'MultiSig' | 'ZkLogin';

export type SignatureFlag = keyof typeof SIGNATURE_FLAG_TO_SCHEME;
