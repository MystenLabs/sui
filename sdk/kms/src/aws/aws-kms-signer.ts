// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { PublicKey, SignatureFlag } from '@mysten/sui/cryptography';
import { SIGNATURE_FLAG_TO_SCHEME, Signer } from '@mysten/sui/cryptography';
import { fromBase64, toBase64 } from '@mysten/sui/utils';
import { secp256r1 } from '@noble/curves/p256';
import { secp256k1 } from '@noble/curves/secp256k1';
import { DERElement } from 'asn1-ts';

import type { AwsClientOptions } from './aws-client.js';
import { AwsKmsClient } from './aws-client.js';

/**
 * Configuration options for initializing the AwsKmsSigner.
 */
export interface AwsKmsSignerOptions {
	/** AWS KMS Key ID used for signing */
	kmsKeyId: string;
	/** Options for setting up the AWS KMS client */
	client: AwsKmsClient;
	/** Public key */
	publicKey: PublicKey;
}

/**
 * Aws KMS Signer integrates AWS Key Management Service (KMS) with the Sui blockchain
 * to provide signing capabilities using AWS-managed cryptographic keys.
 */
export class AwsKmsSigner extends Signer {
	#publicKey: PublicKey;
	/** AWS KMS client instance */
	#client: AwsKmsClient;
	/** AWS KMS Key ID used for signing */
	#kmsKeyId: string;

	/**
	 * Creates an instance of AwsKmsSigner. It's expected to call the static `fromKeyId` method to create an instance.
	 * For example:
	 * ```
	 * const signer = await AwsKmsSigner.fromKeyId(keyId, options);
	 * ```
	 * @throws Will throw an error if required AWS credentials or region are not provided.
	 */
	constructor({ kmsKeyId, client, publicKey }: AwsKmsSignerOptions) {
		super();
		if (!kmsKeyId) throw new Error('KMS Key ID is required');

		this.#client = client;
		this.#kmsKeyId = kmsKeyId;
		this.#publicKey = publicKey;
	}

	/**
	 * Retrieves the key scheme used by this signer.
	 * @returns AWS supports only Secp256k1 and Secp256r1 schemes.
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
	 * Signs the given data using AWS KMS.
	 * @param bytes - The data to be signed as a Uint8Array.
	 * @returns A promise that resolves to the signature as a Uint8Array.
	 * @throws Will throw an error if the public key is not initialized or if signing fails.
	 */
	async sign(bytes: Uint8Array): Promise<Uint8Array> {
		const signResponse = await this.#client.runCommand('Sign', {
			KeyId: this.#kmsKeyId,
			Message: toBase64(bytes),
			MessageType: 'RAW',
			SigningAlgorithm: 'ECDSA_SHA_256',
		});

		// Concatenate the signature components into a compact form
		return this.#getConcatenatedSignature(fromBase64(signResponse.Signature));
	}

	/**
	 * Synchronous signing is not supported by AWS KMS.
	 * @throws Always throws an error indicating synchronous signing is unsupported.
	 */
	signData(): never {
		throw new Error('KMS Signer does not support sync signing');
	}

	/**
	 * Generates a concatenated signature from a DER-encoded signature.
	 *
	 * This signature format is consumable by Sui's `toSerializedSignature` method.
	 *
	 * @param signature - A `Uint8Array` representing the DER-encoded signature.
	 * @returns A `Uint8Array` containing the concatenated signature in compact form.
	 *
	 * @throws {Error} If the input signature is invalid or cannot be processed.
	 */
	#getConcatenatedSignature(signature: Uint8Array): Uint8Array {
		if (!signature || signature.length === 0) {
			throw new Error('Invalid signature');
		}

		// Initialize a DERElement to parse the DER-encoded signature
		const derElement = new DERElement();
		derElement.fromBytes(signature);

		const [r, s] = derElement.toJSON() as [string, string];

		switch (this.getKeyScheme()) {
			case 'Secp256k1':
				return new secp256k1.Signature(BigInt(r), BigInt(s)).normalizeS().toCompactRawBytes();
			case 'Secp256r1':
				return new secp256r1.Signature(BigInt(r), BigInt(s)).normalizeS().toCompactRawBytes();
		}

		// Create a Secp256k1Signature using the extracted r and s values
		const secp256k1Signature = new secp256k1.Signature(BigInt(r), BigInt(s));

		// Normalize the signature and convert it to compact raw bytes
		return secp256k1Signature.normalizeS().toCompactRawBytes();
	}

	/**
	 * Prepares the signer by fetching and setting the public key from AWS KMS.
	 * It is recommended to initialize an `AwsKmsSigner` instance using this function.
	 * @returns A promise that resolves once a `AwsKmsSigner` instance is prepared (public key is set).
	 */
	static async fromKeyId(keyId: string, options: AwsClientOptions) {
		const client = new AwsKmsClient(options);

		const pubKey = await client.getPublicKey(keyId);

		return new AwsKmsSigner({
			kmsKeyId: keyId,
			client,
			publicKey: pubKey,
		});
	}
}
