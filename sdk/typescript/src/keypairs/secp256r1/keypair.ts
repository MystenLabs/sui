// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import { secp256r1 } from '@noble/curves/p256';
import { blake2b } from '@noble/hashes/blake2b';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex } from '@noble/hashes/utils';
import { HDKey } from '@scure/bip32';

import type { ExportedKeypair } from '../../cryptography/keypair.js';
import { Keypair } from '../../cryptography/keypair.js';
import { isValidBIP32Path, mnemonicToSeed } from '../../cryptography/mnemonics.js';
import type { PublicKey } from '../../cryptography/publickey.js';
import type { SignatureScheme } from '../../cryptography/signature-scheme.js';
import { Secp256r1PublicKey } from './publickey.js';

export const DEFAULT_SECP256R1_DERIVATION_PATH = "m/74'/784'/0'/0/0";

/**
 * Secp256r1 Keypair data
 */
export interface Secp256r1KeypairData {
	publicKey: Uint8Array;
	secretKey: Uint8Array;
}

/**
 * An Secp256r1 Keypair used for signing transactions.
 */
export class Secp256r1Keypair extends Keypair {
	private keypair: Secp256r1KeypairData;

	/**
	 * Create a new keypair instance.
	 * Generate random keypair if no {@link Secp256r1Keypair} is provided.
	 *
	 * @param keypair Secp256r1 keypair
	 */
	constructor(keypair?: Secp256r1KeypairData) {
		super();
		if (keypair) {
			this.keypair = keypair;
		} else {
			const secretKey: Uint8Array = secp256r1.utils.randomPrivateKey();
			const publicKey: Uint8Array = secp256r1.getPublicKey(secretKey, true);

			this.keypair = { publicKey, secretKey };
		}
	}

	/**
	 * Get the key scheme of the keypair Secp256r1
	 */
	getKeyScheme(): SignatureScheme {
		return 'Secp256r1';
	}

	/**
	 * Generate a new random keypair
	 */
	static generate(): Secp256r1Keypair {
		return new Secp256r1Keypair();
	}

	/**
	 * Create a keypair from a raw secret key byte array.
	 *
	 * This method should only be used to recreate a keypair from a previously
	 * generated secret key. Generating keypairs from a random seed should be done
	 * with the {@link Keypair.fromSeed} method.
	 *
	 * @throws error if the provided secret key is invalid and validation is not skipped.
	 *
	 * @param secretKey secret key byte array
	 * @param options: skip secret key validation
	 */

	static fromSecretKey(
		secretKey: Uint8Array,
		options?: { skipValidation?: boolean },
	): Secp256r1Keypair {
		const publicKey: Uint8Array = secp256r1.getPublicKey(secretKey, true);
		if (!options || !options.skipValidation) {
			const encoder = new TextEncoder();
			const signData = encoder.encode('sui validation');
			const msgHash = bytesToHex(blake2b(signData, { dkLen: 32 }));
			const signature = secp256r1.sign(msgHash, secretKey, { lowS: true });
			if (!secp256r1.verify(signature, msgHash, publicKey, { lowS: true })) {
				throw new Error('Provided secretKey is invalid');
			}
		}
		return new Secp256r1Keypair({ publicKey, secretKey });
	}

	/**
	 * Generate a keypair from a 32 byte seed.
	 *
	 * @param seed seed byte array
	 */
	static fromSeed(seed: Uint8Array): Secp256r1Keypair {
		let publicKey = secp256r1.getPublicKey(seed, true);
		return new Secp256r1Keypair({ publicKey, secretKey: seed });
	}

	/**
	 * The public key for this keypair
	 */
	getPublicKey(): PublicKey {
		return new Secp256r1PublicKey(this.keypair.publicKey);
	}

	async sign(data: Uint8Array) {
		return this.signData(data);
	}

	/**
	 * Return the signature for the provided data.
	 */
	signData(data: Uint8Array): Uint8Array {
		const msgHash = sha256(data);
		const sig = secp256r1.sign(msgHash, this.keypair.secretKey, {
			lowS: true,
		});
		return sig.toCompactRawBytes();
	}

	/**
	 * Derive Secp256r1 keypair from mnemonics and path. The mnemonics must be normalized
	 * and validated against the english wordlist.
	 *
	 * If path is none, it will default to m/74'/784'/0'/0/0, otherwise the path must
	 * be compliant to BIP-32 in form m/74'/784'/{account_index}'/{change_index}/{address_index}.
	 */
	static deriveKeypair(mnemonics: string, path?: string): Secp256r1Keypair {
		if (path == null) {
			path = DEFAULT_SECP256R1_DERIVATION_PATH;
		}
		if (!isValidBIP32Path(path)) {
			throw new Error('Invalid derivation path');
		}
		// We use HDKey which is hardcoded to use Secp256k1 but since we only need the 32 bytes for the private key it's okay to use here as well.
		const privateKey = HDKey.fromMasterSeed(mnemonicToSeed(mnemonics)).derive(path).privateKey;
		return Secp256r1Keypair.fromSecretKey(privateKey!);
	}

	export(): ExportedKeypair {
		return {
			schema: 'Secp256r1',
			privateKey: toB64(this.keypair.secretKey),
		};
	}
}
