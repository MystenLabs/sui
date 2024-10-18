// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { GetPublicKeyCommand, KMSClient, SignCommand } from '@aws-sdk/client-kms';
import { Signer } from '@mysten/sui/cryptography';
import { Secp256k1PublicKey } from '@mysten/sui/keypairs/secp256k1';
import { ASN1Construction, ASN1TagClass, DERElement } from 'asn1-ts';

import { compressPublicKeyClamped, getConcatenatedSignature } from './utils.js';

/**
 * Configuration options for initializing the AWSKMSSigner.
 */
export interface AWSKMSSignerOptions {
	/** AWS Access Key ID */
	accessKeyId: string;
	/** AWS Secret Access Key */
	secretAccessKey: string;
	/** AWS Region (e.g., 'us-west-2') */
	region: string;
	/** AWS KMS Key ID used for signing */
	kmsKeyId: string;
}

/**
 * AWSKMSSigner integrates AWS Key Management Service (KMS) with the Sui blockchain
 * to provide signing capabilities using AWS-managed cryptographic keys.
 */
export class AWSKMSSigner extends Signer {
	/** The compressed Secp256k1 public key */
	#pubKey?: Secp256k1PublicKey;
	/** AWS KMS client instance */
	#client: KMSClient;
	/** AWS KMS Key ID used for signing */
	#kmsKeyId: string;

	/**
	 * Creates an instance of AWSKMSSigner.
	 * @param options - Configuration options for AWS KMS.
	 * @throws Will throw an error if required AWS credentials or region are not provided.
	 */
	constructor({ accessKeyId, secretAccessKey, region, kmsKeyId }: AWSKMSSignerOptions) {
		super();

		if (!kmsKeyId) {
			throw new Error('KMS Key ID is required');
		}

		this.#kmsKeyId = kmsKeyId;

		const config = {
			region,
			credentials: {
				accessKeyId,
				secretAccessKey,
			},
		};

		if (!config.credentials.accessKeyId || !config.credentials.secretAccessKey) {
			throw new Error(
				'AWS credentials are not set. Please supply the `accessKeyId` and `secretAccessKey`',
			);
		}

		if (!config.region) {
			throw new Error('AWS region is not set');
		}

		// Initialize the AWS KMS client with the provided configuration
		this.#client = new KMSClient(config);
	}

	/**
	 * Prepares the signer by fetching and setting the public key from AWS KMS.
	 * This method must be called before performing any signing operations.
	 * @returns A promise that resolves once the public key is fetched and set.
	 */
	async prepare() {
		this.#pubKey = await this.#getAWSPublicKey();
	}

	/**
	 * Retrieves the key scheme used by this signer.
	 * @returns The string 'Secp256k1' indicating the key scheme.
	 */
	getKeyScheme() {
		return 'Secp256k1' as const;
	}

	/**
	 * Retrieves the public key associated with this signer.
	 * @returns The Secp256k1PublicKey instance.
	 * @throws Will throw an error if the public key has not been initialized.
	 */
	getPublicKey() {
		if (!this.#pubKey) {
			throw new Error('Public key not initialized. Call `prepare` method first.');
		}
		return this.#pubKey;
	}

	/**
	 * Signs the given data using AWS KMS.
	 * @param bytes - The data to be signed as a Uint8Array.
	 * @returns A promise that resolves to the signature as a Uint8Array.
	 * @throws Will throw an error if the public key is not initialized or if signing fails.
	 */
	async sign(bytes: Uint8Array): Promise<Uint8Array> {
		if (!this.#pubKey) {
			throw new Error('Public key not initialized. Call `prepare` method first.');
		}

		const signCommand = new SignCommand({
			KeyId: this.#kmsKeyId,
			Message: bytes,
			MessageType: 'RAW',
			SigningAlgorithm: 'ECDSA_SHA_256', // Adjust the algorithm based on your key spec
		});

		const signResponse = await this.#client.send(signCommand);

		if (!signResponse.Signature) {
			throw new Error('Signature not found in the response. Execution failed.');
		}

		// Concatenate the signature components into a compact form
		const compactSignature = getConcatenatedSignature(signResponse.Signature);

		return compactSignature;
	}

	/**
	 * Synchronous signing is not supported by AWS KMS.
	 * @throws Always throws an error indicating synchronous signing is unsupported.
	 */
	signData(): never {
		throw new Error('KMS Signer does not support sync signing');
	}

	/**
	 * Fetches the AWS KMS public key and converts it to the Sui Secp256k1PublicKey format.
	 * @private
	 * @returns A promise that resolves to a Secp256k1PublicKey instance.
	 * @throws Will throw an error if the public key cannot be retrieved or parsed.
	 */
	async #getAWSPublicKey() {
		// Create a command to get the public key from AWS KMS
		const getPublicKeyCommand = new GetPublicKeyCommand({
			KeyId: this.#kmsKeyId,
		});

		const publicKeyResponse = await this.#client.send(getPublicKeyCommand);

		if (!publicKeyResponse.PublicKey) {
			throw new Error('Public Key not found for the supplied `keyId`');
		}

		const publicKey = publicKeyResponse.PublicKey;
		const derElement = new DERElement();
		derElement.fromBytes(publicKey);

		// Validate the ASN.1 structure of the public key
		if (
			!(
				derElement.tagClass === ASN1TagClass.universal &&
				derElement.construction === ASN1Construction.constructed
			)
		) {
			throw new Error('Unexpected ASN.1 structure');
		}

		const components = derElement.components;
		const publicKeyElement = components[1];

		if (!publicKeyElement) {
			throw new Error('Public Key not found in the DER structure');
		}

		// Extract the raw public key from the ASN.1 bit string
		const rawPublicKey = publicKeyElement.bitString;
		const compressedKey = compressPublicKeyClamped(rawPublicKey);

		// Convert the compressed key to the Secp256k1PublicKey format used by Sui
		return new Secp256k1PublicKey(compressedKey);
	}
}
