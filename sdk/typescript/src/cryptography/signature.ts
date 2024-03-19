// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { MultiSigStruct } from '../multisig/publickey.js';
import { computeZkLoginAddressFromSeed } from '../zklogin/address.js';
import { extractClaimValue } from '../zklogin/jwt-utils.js';
import { parseZkLoginSignature } from '../zklogin/signature.js';
import type { PublicKey } from './publickey.js';
import type { SignatureScheme } from './signature-scheme.js';
import {
	SIGNATURE_FLAG_TO_SCHEME,
	SIGNATURE_SCHEME_TO_FLAG,
	SIGNATURE_SCHEME_TO_SIZE,
} from './signature-scheme.js';

/**
 * Pair of signature and corresponding public key
 */
export type SerializeSignatureInput = {
	signatureScheme: SignatureScheme;
	/** Base64-encoded signature */
	signature: Uint8Array;
	/** Base64-encoded public key */
	publicKey?: PublicKey;
};

/**
 * (`flag || signature || pubkey` bytes, as base-64 encoded string).
 * Signature is committed to the intent message of the transaction data, as base-64 encoded string.
 */
export type SerializedSignature = string;

/**
 * Takes in a signature, its associated signing scheme and a public key, then serializes this data
 */
export function toSerializedSignature({
	signature,
	signatureScheme,
	publicKey,
}: SerializeSignatureInput): SerializedSignature {
	if (!publicKey) {
		throw new Error('`publicKey` is required');
	}

	const pubKeyBytes = publicKey.toRawBytes();
	const serializedSignature = new Uint8Array(1 + signature.length + pubKeyBytes.length);
	serializedSignature.set([SIGNATURE_SCHEME_TO_FLAG[signatureScheme]]);
	serializedSignature.set(signature, 1);
	serializedSignature.set(pubKeyBytes, 1 + signature.length);
	return toB64(serializedSignature);
}

/**
 * Decodes a serialized signature into its constituent components: the signature scheme, the actual signature, and the public key
 */
export function parseSerializedSignature(serializedSignature: SerializedSignature) {
	const bytes = fromB64(serializedSignature);

	const signatureScheme =
		SIGNATURE_FLAG_TO_SCHEME[bytes[0] as keyof typeof SIGNATURE_FLAG_TO_SCHEME];

	switch (signatureScheme) {
		case 'MultiSig':
			const multisig: MultiSigStruct = bcs.MultiSig.parse(bytes.slice(1));
			return {
				serializedSignature,
				signatureScheme,
				multisig,
				bytes,
			};
		case 'ZkLogin':
			const signatureBytes = bytes.slice(1);
			const { inputs, maxEpoch, userSignature } = parseZkLoginSignature(signatureBytes);
			const { issBase64Details, addressSeed } = inputs;
			const iss = extractClaimValue<string>(issBase64Details, 'iss');
			const address = computeZkLoginAddressFromSeed(BigInt(addressSeed), iss);

			return {
				serializedSignature,
				signatureScheme,
				zkLogin: {
					inputs,
					maxEpoch,
					userSignature,
					iss,
					address,
					addressSeed: BigInt(addressSeed),
				},
				signature: bytes,
			};
		case 'ED25519':
		case 'Secp256k1':
		case 'Secp256r1':
			const size =
				SIGNATURE_SCHEME_TO_SIZE[signatureScheme as keyof typeof SIGNATURE_SCHEME_TO_SIZE];
			const signature = bytes.slice(1, bytes.length - size);
			const publicKey = bytes.slice(1 + signature.length);

			return {
				serializedSignature,
				signatureScheme,
				signature,
				publicKey,
				bytes,
			};
		default:
			throw new Error('Unsupported signature scheme');
	}
}
