// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64 } from '@mysten/bcs';

import type { PublicKey, SignatureFlag, SignatureScheme } from '../cryptography/index.js';
import { parseSerializedSignature, SIGNATURE_FLAG_TO_SCHEME } from '../cryptography/index.js';
import type { SuiGraphQLClient } from '../graphql/client.js';
import { Ed25519PublicKey } from '../keypairs/ed25519/publickey.js';
import { Secp256k1PublicKey } from '../keypairs/secp256k1/publickey.js';
import { Secp256r1PublicKey } from '../keypairs/secp256r1/publickey.js';
// eslint-disable-next-line import/no-cycle
import { MultiSigPublicKey } from '../multisig/publickey.js';
import { ZkLoginPublicIdentifier } from '../zklogin/publickey.js';

export async function verifySignature(bytes: Uint8Array, signature: string): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (!(await parsedSignature.publicKey.verify(bytes, parsedSignature.serializedSignature))) {
		throw new Error(`Signature is not valid for the provided data`);
	}

	return parsedSignature.publicKey;
}

export async function verifyPersonalMessageSignature(
	message: Uint8Array,
	signature: string,
	options: { client?: SuiGraphQLClient } = {},
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature, options);

	if (
		!(await parsedSignature.publicKey.verifyPersonalMessage(
			message,
			parsedSignature.serializedSignature,
		))
	) {
		throw new Error(`Signature is not valid for the provided message`);
	}

	return parsedSignature.publicKey;
}

export async function verifyTransactionSignature(
	transaction: Uint8Array,
	signature: string,
	options: { client?: SuiGraphQLClient } = {},
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature, options);

	if (
		!(await parsedSignature.publicKey.verifyTransaction(
			transaction,
			parsedSignature.serializedSignature,
		))
	) {
		throw new Error(`Signature is not valid for the provided Transaction`);
	}

	return parsedSignature.publicKey;
}

function parseSignature(signature: string, options: { client?: SuiGraphQLClient } = {}) {
	const parsedSignature = parseSerializedSignature(signature);

	if (parsedSignature.signatureScheme === 'MultiSig') {
		return {
			...parsedSignature,
			publicKey: new MultiSigPublicKey(parsedSignature.multisig.multisig_pk),
		};
	}

	const publicKey = publicKeyFromRawBytes(
		parsedSignature.signatureScheme,
		parsedSignature.publicKey,
		options,
	);
	return {
		...parsedSignature,
		publicKey,
	};
}

export function publicKeyFromRawBytes(
	signatureScheme: SignatureScheme,
	bytes: Uint8Array,
	options: { client?: SuiGraphQLClient } = {},
): PublicKey {
	switch (signatureScheme) {
		case 'ED25519':
			return new Ed25519PublicKey(bytes);
		case 'Secp256k1':
			return new Secp256k1PublicKey(bytes);
		case 'Secp256r1':
			return new Secp256r1PublicKey(bytes);
		case 'MultiSig':
			return new MultiSigPublicKey(bytes);
		case 'ZkLogin':
			return new ZkLoginPublicIdentifier(bytes, options);
		default:
			throw new Error(`Unsupported signature scheme ${signatureScheme}`);
	}
}

export function publicKeyFromSuiBytes(
	publicKey: string | Uint8Array,
	options: { client?: SuiGraphQLClient } = {},
) {
	const bytes = typeof publicKey === 'string' ? fromBase64(publicKey) : publicKey;

	const signatureScheme = SIGNATURE_FLAG_TO_SCHEME[bytes[0] as SignatureFlag];

	return publicKeyFromRawBytes(signatureScheme, bytes.slice(1), options);
}
