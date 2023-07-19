// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import type { PublicKey } from './publickey.js';
import type { MultiSigStruct } from '../multisig/publickey.js';
import { builder } from '../builder/bcs.js';

export type SignatureScheme = 'ED25519' | 'Secp256k1' | 'Secp256r1' | 'MultiSig';

/**
 * Pair of signature and corresponding public key
 */
export type SerializeSignatureInput = {
	signatureScheme: SignatureScheme;
	/** Base64-encoded signature */
	signature: Uint8Array;
	/** @deprecated use publicKey instead */
	pubKey?: PublicKey;
	/** Base64-encoded public key */
	publicKey?: PublicKey;
};

/**
 * (`flag || signature || pubkey` bytes, as base-64 encoded string).
 * Signature is committed to the intent message of the transaction data, as base-64 encoded string.
 */
export type SerializedSignature = string;

export const SIGNATURE_SCHEME_TO_FLAG = {
	ED25519: 0x00,
	Secp256k1: 0x01,
	Secp256r1: 0x02,
	MultiSig: 0x03,
};

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
} as const;

export type SignatureFlag = keyof typeof SIGNATURE_FLAG_TO_SCHEME;

export function toSerializedSignature({
	signature,
	signatureScheme,
	pubKey,
	publicKey = pubKey,
}: SerializeSignatureInput): SerializedSignature {
	if (!publicKey) {
		throw new Error('`publicKey` is required');
	}

	const pubKeyBytes = publicKey.toBytes();
	const serializedSignature = new Uint8Array(1 + signature.length + pubKeyBytes.length);
	serializedSignature.set([SIGNATURE_SCHEME_TO_FLAG[signatureScheme]]);
	serializedSignature.set(signature, 1);
	serializedSignature.set(pubKeyBytes, 1 + signature.length);
	return toB64(serializedSignature);
}

export function parseSerializedSignature(serializedSignature: SerializedSignature) {
	const bytes = fromB64(serializedSignature);

	const signatureScheme =
		SIGNATURE_FLAG_TO_SCHEME[bytes[0] as keyof typeof SIGNATURE_FLAG_TO_SCHEME];

	if (signatureScheme === 'MultiSig') {
		const multisig: MultiSigStruct = builder.de('MultiSig', bytes.slice(1));
		return {
			serializedSignature,
			signatureScheme,
			multisig,
			bytes,
		};
	}

	if (!(signatureScheme in SIGNATURE_SCHEME_TO_SIZE)) {
		throw new Error('Unsupported signature scheme');
	}

	const size = SIGNATURE_SCHEME_TO_SIZE[signatureScheme as keyof typeof SIGNATURE_SCHEME_TO_SIZE];

	const signature = bytes.slice(1, bytes.length - size);
	const publicKey = bytes.slice(1 + signature.length);

	return {
		serializedSignature,
		signatureScheme,
		signature,
		publicKey,
		bytes,
	};
}
