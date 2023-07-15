// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey, SerializedSignature } from '../cryptography/index.js';
import { parseSerializedSignature } from '../cryptography/index.js';
import { Ed25519PublicKey } from '../keypairs/ed25519/publickey.js';
import { Secp256k1PublicKey } from '../keypairs/secp256k1/publickey.js';
import { Secp256r1PublicKey } from '../keypairs/secp256r1/publickey.js';

export async function verifySignature(
	bytes: Uint8Array,
	signature: SerializedSignature,
): Promise<false | PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (await parsedSignature.publicKey.verify(bytes, parsedSignature.signature)) {
		return parsedSignature.publicKey;
	}

	return false;
}

export async function verifyPersonalMessage(
	message: Uint8Array,
	signature: SerializedSignature,
): Promise<false | PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (await parsedSignature.publicKey.verifyPersonalMessage(message, parsedSignature.signature)) {
		return parsedSignature.publicKey;
	}

	return false;
}

export async function verifyTransactionBlock(
	transactionBlock: Uint8Array,
	signature: SerializedSignature,
): Promise<false | PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (
		await parsedSignature.publicKey.verifyTransactionBlock(
			transactionBlock,
			parsedSignature.signature,
		)
	) {
		return parsedSignature.publicKey;
	}

	return false;
}

function parseSignature(signature: SerializedSignature) {
	const parsedSignature = parseSerializedSignature(signature);
	let publicKey: PublicKey;

	switch (parsedSignature.signatureScheme) {
		case 'ED25519':
			publicKey = Ed25519PublicKey.fromBytes(parsedSignature.publicKey);
			break;
		case 'Secp256k1':
			publicKey = Secp256k1PublicKey.fromBytes(parsedSignature.publicKey);
			break;
		case 'Secp256r1':
			publicKey = Secp256r1PublicKey.fromBytes(parsedSignature.publicKey);
			break;
		default:
			throw new Error(`Unsupported signature scheme ${parsedSignature.signatureScheme}`);
	}

	return {
		...parsedSignature,
		publicKey,
	};
}
