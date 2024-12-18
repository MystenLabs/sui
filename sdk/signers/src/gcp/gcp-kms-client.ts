// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { KeyManagementServiceClient } from '@google-cloud/kms';
import type { PublicKey, SignatureFlag } from '@mysten/sui/cryptography';
import { SIGNATURE_FLAG_TO_SCHEME, Signer } from '@mysten/sui/cryptography';
import { Secp256k1PublicKey } from '@mysten/sui/keypairs/secp256k1';
import { Secp256r1PublicKey } from '@mysten/sui/keypairs/secp256r1';
import { fromBase64 } from '@mysten/sui/utils';

import { getConcatenatedSignature, publicKeyFromDER } from '../utils/utils.js';

/**
 * Configuration options for initializing the GcpKmsSigner.
 */
export interface GcpKmsSignerOptions {
	/** The version name generated from `client.cryptoKeyVersionPath()` */
	versionName: string;
	/** Options for setting up the GCP KMS client */
	client: KeyManagementServiceClient;
	/** Public key */
	publicKey: PublicKey;
}

/**
 * GCP KMS Signer integrates GCP Key Management Service (KMS) with the Sui blockchain
 * to provide signing capabilities using GCP-managed cryptographic keys.
 */
export class GcpKmsSigner extends Signer {
	#publicKey: PublicKey;
	/** GCP KMS client instance */
	#client: KeyManagementServiceClient;
	/** GCP KMS version name (generated from `client.cryptoKeyVersionPath()`) */
	#versionName: string;

	/**
	 * Creates an instance of GcpKmsSigner. It's expected to call the static `fromOptions`
	 * or `fromVersionName` method to create an instance.
	 * For example:
	 * ```
	 * const signer = await GcpKmsSigner.fromVersionName(versionName);
	 * ```
	 * @throws Will throw an error if required GCP credentials are not provided.
	 */
	constructor({ versionName, client, publicKey }: GcpKmsSignerOptions) {
		super();
		if (!versionName) throw new Error('Version name is required');

		this.#client = client;
		this.#versionName = versionName;
		this.#publicKey = publicKey;
	}

	/**
	 * Retrieves the key scheme used by this signer.
	 * @returns GCP supports only `Secp256k1` and `Secp256r1` schemes.
	 */
	getKeyScheme() {
		return SIGNATURE_FLAG_TO_SCHEME[this.#publicKey.flag() as SignatureFlag];
	}

	/**
	 * Retrieves the public key associated with this signer.
	 * @returns The Secp256k1PublicKey instance.
	 * @throws Will throw an error if the public key has not been initialized.
	 */
	getPublicKey() {
		return this.#publicKey;
	}

	/**
	 * Signs the given data using GCP KMS.
	 * @param bytes - The data to be signed as a Uint8Array.
	 * @returns A promise that resolves to the signature as a Uint8Array.
	 * @throws Will throw an error if the public key is not initialized or if signing fails.
	 */
	async sign(bytes: Uint8Array): Promise<Uint8Array> {
		const [signResponse] = await this.#client.asymmetricSign({
			name: this.#versionName,
			data: bytes,
		});

		if (!signResponse.signature) {
			throw new Error('No signature returned from GCP KMS');
		}

		return getConcatenatedSignature(signResponse.signature as Uint8Array, this.getKeyScheme());
	}

	/**
	 * Synchronous signing is not supported by GCP KMS.
	 * @throws Always throws an error indicating synchronous signing is unsupported.
	 */
	signData(): never {
		throw new Error('GCP Signer does not support sync signing');
	}

	/**
	 * Creates a GCP KMS signer from the provided options.
	 * Expects the credentials file to be set as an env variable
	 * (GOOGLE_APPLICATION_CREDENTIALS).
	 */
	static async fromOptions(options: {
		projectId: string;
		location: string;
		keyRing: string;
		cryptoKey: string;
		cryptoKeyVersion: string;
	}) {
		const client = new KeyManagementServiceClient();

		const versionName = client.cryptoKeyVersionPath(
			options.projectId,
			options.location,
			options.keyRing,
			options.cryptoKey,
			options.cryptoKeyVersion,
		);

		return new GcpKmsSigner({
			versionName,
			client,
			publicKey: await getPublicKey(client, versionName),
		});
	}

	static async fromVersionName(versionName: string) {
		const client = new KeyManagementServiceClient();
		return new GcpKmsSigner({
			versionName,
			client,
			publicKey: await getPublicKey(client, versionName),
		});
	}
}

/**
 * Retrieves the public key associated with the given version name.
 */
async function getPublicKey(
	client: KeyManagementServiceClient,
	versionName: string,
): Promise<PublicKey> {
	const [publicKey] = await client.getPublicKey({ name: versionName });

	const { algorithm, pem } = publicKey;

	if (!pem) throw new Error('No PEM key returned from GCP KMS');

	const base64 = pem
		.replace('-----BEGIN PUBLIC KEY-----', '')
		.replace('-----END PUBLIC KEY-----', '')
		.replace(/\s/g, '');

	const compressedKey = publicKeyFromDER(fromBase64(base64));

	switch (algorithm) {
		case 'EC_SIGN_SECP256K1_SHA256':
			return new Secp256k1PublicKey(compressedKey);
		case 'EC_SIGN_P256_SHA256':
			return new Secp256r1PublicKey(compressedKey);
		default:
			throw new Error(`Unsupported algorithm: ${algorithm}`);
	}
}
