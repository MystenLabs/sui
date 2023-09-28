// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';

import { bcs } from '../bcs/index.js';
import { IntentScope, messageWithIntent } from '../cryptography/intent.js';
import type { SerializedSignature } from '../cryptography/signature.js';
import { toSingleSignaturePubkeyPair } from '../cryptography/utils.js';

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
			bcs
				.vector(bcs.u8())
				.serialize(typeof message === 'string' ? fromB64(message) : message)
				.toBytes(),
		);

		if (await signature.pubKey.verify(blake2b(messageBytes, { dkLen: 32 }), signature.signature)) {
			return true;
		}

		// Fallback for backwards compatibility, old versions of the SDK
		// did not properly wrap PersonalMessages in a PersonalMessage bcs Struct
		const unwrappedMessageBytes = messageWithIntent(
			scope,
			typeof message === 'string' ? fromB64(message) : message,
		);

		return signature.pubKey.verify(
			blake2b(unwrappedMessageBytes, { dkLen: 32 }),
			signature.signature,
		);
	}

	const messageBytes = messageWithIntent(
		scope,
		typeof message === 'string' ? fromB64(message) : message,
	);

	return signature.pubKey.verify(blake2b(messageBytes, { dkLen: 32 }), signature.signature);
}
