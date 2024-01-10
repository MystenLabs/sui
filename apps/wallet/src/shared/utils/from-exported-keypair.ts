// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExportedKeypair, type Keypair } from '@mysten/sui.js/cryptography';
import {
	decodeSuiPrivateKey,
	LEGACY_PRIVATE_KEY_SIZE,
	PRIVATE_KEY_SIZE,
} from '@mysten/sui.js/cryptography/keypair';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { Secp256k1Keypair } from '@mysten/sui.js/keypairs/secp256k1';
import { Secp256r1Keypair } from '@mysten/sui.js/keypairs/secp256r1';

export function validateExportedKeypair(keypair: ExportedKeypair): ExportedKeypair {
	const _kp = decodeSuiPrivateKey(keypair.privateKey);
	return keypair;
}

export function fromExportedKeypair(keypair: ExportedKeypair): Keypair {
	const { schema, secretKey } = decodeSuiPrivateKey(keypair.privateKey);

	switch (schema) {
		case 'ED25519':
			let pureSecretKey = secretKey;
			if (secretKey.length === LEGACY_PRIVATE_KEY_SIZE) {
				// This is a legacy secret key, we need to strip the public key bytes and only read the first 32 bytes
				pureSecretKey = secretKey.slice(0, PRIVATE_KEY_SIZE);
			}
			return Ed25519Keypair.fromSecretKey(pureSecretKey);
		case 'Secp256k1':
			return Secp256k1Keypair.fromSecretKey(secretKey);
		case 'Secp256r1':
			return Secp256r1Keypair.fromSecretKey(secretKey);
		default:
			throw new Error(`Invalid keypair schema ${keypair.schema}`);
	}
}
