// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import nacl from 'tweetnacl';
import { IntentScope } from '../cryptography/intent.js';
import { messageWithIntent } from '../cryptography/intent.js';
import { secp256k1 } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import type { SerializedSignature, SignatureScheme } from '../cryptography/signature.js';
import { blake2b } from '@noble/hashes/blake2b';
import { toSingleSignaturePubkeyPair } from '../cryptography/utils.js';
import { bcs } from '../types/sui-bcs.js';

// TODO: This might actually make sense to eventually move to the `Keypair` instances themselves, as
// it could allow the Sui.js to be tree-shaken a little better, possibly allowing keypairs that are
// not used (and their deps) to be entirely removed from the bundle.

/** Verify data that is signed with the expected scope. */
export async function verifyMessage(
	message: Uint8Array | string,
	serializedSignature: SerializedSignature,
	scope: IntentScope,
) {
	const signature = toSingleSignaturePubkeyPair(serializedSignature);

	if (scope === IntentScope.PersonalMessage) {
		const messageBytes = messageWithIntent(
			scope,
			bcs.ser(['vector', 'u8'], typeof message === 'string' ? fromB64(message) : message).toBytes(),
		);

		if (
			verifySignature(
				blake2b(messageBytes, { dkLen: 32 }),
				signature.signature,
				signature.pubKey.toBytes(),
				signature.signatureScheme,
			)
		) {
			return true;
		}

		// Fallback for backwards compatibility, old versions of the SDK
		// did not properly wrap PersonalMessages in a PersonalMessage bcs Struct
		const unwrappedMessageBytes = messageWithIntent(
			scope,
			typeof message === 'string' ? fromB64(message) : message,
		);

		return verifySignature(
			blake2b(unwrappedMessageBytes, { dkLen: 32 }),
			signature.signature,
			signature.pubKey.toBytes(),
			signature.signatureScheme,
		);
	}

	const messageBytes = messageWithIntent(
		scope,
		typeof message === 'string' ? fromB64(message) : message,
	);

	return verifySignature(
		blake2b(messageBytes, { dkLen: 32 }),
		signature.signature,
		signature.pubKey.toBytes(),
		signature.signatureScheme,
	);
}

function verifySignature(
	bytes: Uint8Array,
	signature: Uint8Array,
	publicKey: Uint8Array,
	signatureScheme: SignatureScheme,
) {
	switch (signatureScheme) {
		case 'ED25519':
			return nacl.sign.detached.verify(bytes, signature, publicKey);
		case 'Secp256k1':
			return secp256k1.verify(secp256k1.Signature.fromCompact(signature), sha256(bytes), publicKey);
		default:
			throw new Error(`Unknown signature scheme: "${signatureScheme}"`);
	}
}
