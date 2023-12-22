// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExportedKeypair, type Keypair } from '@mysten/sui.js/cryptography';
import { SUI_PRIVATE_KEY_PREFIX } from '@mysten/sui.js/cryptography/keypair';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { Secp256k1Keypair } from '@mysten/sui.js/keypairs/secp256k1';
import { Secp256r1Keypair } from '@mysten/sui.js/keypairs/secp256r1';
import { fromB64 } from '@mysten/sui.js/utils';
import {bech32} from 'bech32';

const PRIVATE_KEY_SIZE = 32;
const LEGACY_PRIVATE_KEY_SIZE = 64;
export function fromExportedKeypair(keypair: ExportedKeypair): Keypair {
	const {prefix, words} = bech32.decode(keypair.privateKey);
	const secretKey = new Uint8Array(bech32.fromWords(words));

	if (prefix != SUI_PRIVATE_KEY_PREFIX) {
		throw new Error('invalid key');
	}

	switch (keypair.schema) {
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
