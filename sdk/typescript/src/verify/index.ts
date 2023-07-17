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
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (!(await parsedSignature.publicKey.verify(bytes, parsedSignature.signature))) {
		throw new Error(`Signature is not valid for the provided data`);
	}

	return parsedSignature.publicKey;
}

export async function verifyPersonalMessage(
	message: Uint8Array,
	signature: SerializedSignature,
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (
		!(await parsedSignature.publicKey.verifyPersonalMessage(message, parsedSignature.signature))
	) {
		throw new Error(`Signature is not valid for the provided message`);
	}

	return parsedSignature.publicKey;
}

export async function verifyTransactionBlock(
	transactionBlock: Uint8Array,
	signature: SerializedSignature,
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (
		!(await parsedSignature.publicKey.verifyTransactionBlock(
			transactionBlock,
			parsedSignature.signature,
		))
	) {
		throw new Error(`Signature is not valid for the provided TransactionBlock`);
	}

	return parsedSignature.publicKey;
}

function parseSignature(signature: SerializedSignature) {
	const parsedSignature = parseSerializedSignature(signature);
	let publicKey: PublicKey;

	switch (parsedSignature.signatureScheme) {
		case 'ED25519':
			publicKey = new Ed25519PublicKey(parsedSignature.publicKey);
			break;
		case 'Secp256k1':
			publicKey = new Secp256k1PublicKey(parsedSignature.publicKey);
			break;
		case 'Secp256r1':
			publicKey = new Secp256r1PublicKey(parsedSignature.publicKey);
			break;
		default:
			throw new Error(`Unsupported signature scheme ${parsedSignature.signatureScheme}`);
	}

	return {
		...parsedSignature,
		publicKey,
	};
}
