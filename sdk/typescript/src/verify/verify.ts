// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';

import type { PublicKey, SignatureFlag, SignatureScheme } from '../cryptography/index.js';
import { parseSerializedSignature, SIGNATURE_FLAG_TO_SCHEME } from '../cryptography/index.js';
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
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature);

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

export async function verifyTransactionBlockSignature(
	transactionBlock: Uint8Array,
	signature: string,
): Promise<PublicKey> {
	const parsedSignature = parseSignature(signature);

	if (
		!(await parsedSignature.publicKey.verifyTransactionBlock(
			transactionBlock,
			parsedSignature.serializedSignature,
		))
	) {
		throw new Error(`Signature is not valid for the provided TransactionBlock`);
	}

	return parsedSignature.publicKey;
}

function parseSignature(signature: string) {
	const parsedSignature = parseSerializedSignature(signature);

	if (parsedSignature.signatureScheme === 'MultiSig') {
		return {
			...parsedSignature,
			publicKey: new MultiSigPublicKey(parsedSignature.multisig.multisig_pk),
		};
	}

	if (parsedSignature.signatureScheme === 'ZkLogin') {
		throw new Error('ZkLogin is not supported yet');
	}

	const publicKey = publicKeyFromRawBytes(
		parsedSignature.signatureScheme,
		parsedSignature.publicKey,
	);
	return {
		...parsedSignature,
		publicKey,
	};
}

export function publicKeyFromRawBytes(
	signatureScheme: SignatureScheme,
	bytes: Uint8Array,
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
			return new ZkLoginPublicIdentifier(bytes);
		default:
			throw new Error(`Unsupported signature scheme ${signatureScheme}`);
	}
}

export function publicKeyFromSuiBytes(publicKey: string | Uint8Array) {
	const bytes = typeof publicKey === 'string' ? fromB64(publicKey) : publicKey;

	const signatureScheme = SIGNATURE_FLAG_TO_SCHEME[bytes[0] as SignatureFlag];

	return publicKeyFromRawBytes(signatureScheme, bytes.slice(1));
}
