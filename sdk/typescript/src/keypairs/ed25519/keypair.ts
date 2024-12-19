// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ed25519 } from '@noble/curves/ed25519';

import {
	decodeSuiPrivateKey,
	encodeSuiPrivateKey,
	Keypair,
	PRIVATE_KEY_SIZE,
} from '../../cryptography/keypair.js';
import { isValidHardenedPath, mnemonicToSeedHex } from '../../cryptography/mnemonics.js';
import type { SignatureScheme } from '../../cryptography/signature-scheme.js';
import { derivePath } from './ed25519-hd-key.js';
import { Ed25519PublicKey } from './publickey.js';

export const DEFAULT_ED25519_DERIVATION_PATH = "m/44'/784'/0'/0'/0'";

/**
 * Ed25519 Keypair data. The publickey is the 32-byte public key and
 * the secretkey is 64-byte, where the first 32 bytes is the secret
 * key and the last 32 bytes is the public key.
 */
export interface Ed25519KeypairData {
	publicKey: Uint8Array;
	secretKey: Uint8Array;
}

/**
 * An Ed25519 Keypair used for signing transactions.
 */
export class Ed25519Keypair extends Keypair {
	private keypair: Ed25519KeypairData;

	/**
	 * Create a new Ed25519 keypair instance.
	 * Generate random keypair if no {@link Ed25519Keypair} is provided.
	 *
	 * @param keypair Ed25519 keypair
	 */
	constructor(keypair?: Ed25519KeypairData) {
		super();
		if (keypair) {
			this.keypair = {
				publicKey: keypair.publicKey,
				secretKey: keypair.secretKey.slice(0, 32),
			};
		} else {
			const privateKey = ed25519.utils.randomPrivateKey();
			this.keypair = {
				publicKey: ed25519.getPublicKey(privateKey),
				secretKey: privateKey,
			};
		}
	}

	/**
	 * Get the key scheme of the keypair ED25519
	 */
	getKeyScheme(): SignatureScheme {
		return 'ED25519';
	}

	/**
	 * Generate a new random Ed25519 keypair
	 */
	static generate(): Ed25519Keypair {
		const secretKey = ed25519.utils.randomPrivateKey();
		return new Ed25519Keypair({
			publicKey: ed25519.getPublicKey(secretKey),
			secretKey,
		});
	}

	/**
	 * Create a Ed25519 keypair from a raw secret key byte array, also known as seed.
	 * This is NOT the private scalar which is result of hashing and bit clamping of
	 * the raw secret key.
	 *
	 * @throws error if the provided secret key is invalid and validation is not skipped.
	 *
	 * @param secretKey secret key as a byte array or Bech32 secret key string
	 * @param options: skip secret key validation
	 */
	static fromSecretKey(
		secretKey: Uint8Array | string,
		options?: { skipValidation?: boolean },
	): Ed25519Keypair {
		if (typeof secretKey === 'string') {
			const decoded = decodeSuiPrivateKey(secretKey);

			if (decoded.schema !== 'ED25519') {
				throw new Error(`Expected a ED25519 keypair, got ${decoded.schema}`);
			}

			return this.fromSecretKey(decoded.secretKey, options);
		}

		const secretKeyLength = secretKey.length;
		if (secretKeyLength !== PRIVATE_KEY_SIZE) {
			throw new Error(
				`Wrong secretKey size. Expected ${PRIVATE_KEY_SIZE} bytes, got ${secretKeyLength}.`,
			);
		}
		const keypair = {
			publicKey: ed25519.getPublicKey(secretKey),
			secretKey,
		};

		if (!options || !options.skipValidation) {
			const encoder = new TextEncoder();
			const signData = encoder.encode('sui validation');
			const signature = ed25519.sign(signData, secretKey);
			if (!ed25519.verify(signature, signData, keypair.publicKey)) {
				throw new Error('provided secretKey is invalid');
			}
		}
		return new Ed25519Keypair(keypair);
	}

	/**
	 * The public key for this Ed25519 keypair
	 */
	getPublicKey(): Ed25519PublicKey {
		return new Ed25519PublicKey(this.keypair.publicKey);
	}

	/**
	 * The Bech32 secret key string for this Ed25519 keypair
	 */
	getSecretKey(): string {
		return encodeSuiPrivateKey(
			this.keypair.secretKey.slice(0, PRIVATE_KEY_SIZE),
			this.getKeyScheme(),
		);
	}

	/**
	 * Return the signature for the provided data using Ed25519.
	 */
	async sign(data: Uint8Array) {
		return ed25519.sign(data, this.keypair.secretKey);
	}

	/**
	 * Derive Ed25519 keypair from mnemonics and path. The mnemonics must be normalized
	 * and validated against the english wordlist.
	 *
	 * If path is none, it will default to m/44'/784'/0'/0'/0', otherwise the path must
	 * be compliant to SLIP-0010 in form m/44'/784'/{account_index}'/{change_index}'/{address_index}'.
	 */
	static deriveKeypair(mnemonics: string, path?: string): Ed25519Keypair {
		if (path == null) {
			path = DEFAULT_ED25519_DERIVATION_PATH;
		}
		if (!isValidHardenedPath(path)) {
			throw new Error('Invalid derivation path');
		}
		const { key } = derivePath(path, mnemonicToSeedHex(mnemonics));

		return Ed25519Keypair.fromSecretKey(key);
	}

	/**
	 * Derive Ed25519 keypair from mnemonicSeed and path.
	 *
	 * If path is none, it will default to m/44'/784'/0'/0'/0', otherwise the path must
	 * be compliant to SLIP-0010 in form m/44'/784'/{account_index}'/{change_index}'/{address_index}'.
	 */
	static deriveKeypairFromSeed(seedHex: string, path?: string): Ed25519Keypair {
		if (path == null) {
			path = DEFAULT_ED25519_DERIVATION_PATH;
		}
		if (!isValidHardenedPath(path)) {
			throw new Error('Invalid derivation path');
		}
		const { key } = derivePath(path, seedHex);

		return Ed25519Keypair.fromSecretKey(key);
	}
}
