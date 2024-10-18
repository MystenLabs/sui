// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { GetPublicKeyCommand, KMSClient, SignCommand } from '@aws-sdk/client-kms';
import { Signer } from '@mysten/sui/cryptography';
import { Secp256k1PublicKey } from '@mysten/sui/keypairs/secp256k1';
import { ASN1Construction, ASN1TagClass, DERElement } from 'asn1-ts';

import { compressPublicKeyClamped, getConcatenatedSignature } from './utils.js';

/**
 * TODO: Add comments :)
 */
export interface AWSKMSSignerOptions {
	accessKeyId: string;
	secretAccessKey: string;
	region: string;
	kmsKeyId: string;
}

export class AWSKMSSigner extends Signer {
	#pubKey?: Secp256k1PublicKey;
	#client: KMSClient;
	#kmsKeyId: string;

	constructor({ accessKeyId, secretAccessKey, region, kmsKeyId }: AWSKMSSignerOptions) {
		super();
		if (!kmsKeyId) throw new Error('KMS Key ID is required');

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
		// this.#pk = new Secp256k1PublicKey(publicKey);
		this.#client = new KMSClient(config);
	}

	/// This method is called by the `Signer` class to prepare the signer for signing.
	/// This method should be called before any other method.
	/// This method is async because it needs to fetch the public key from AWS.
	async prepare() {
		this.#pubKey = await this.#getAWSPublicKey();
	}

	getKeyScheme() {
		return 'Secp256k1' as const;
	}

	getPublicKey() {
		if (!this.#pubKey) {
			throw new Error('Public key not initialized. Call `prepare` method first.');
		}
		return this.#pubKey;
	}

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
		const compactSignature = getConcatenatedSignature(signResponse.Signature);

		return compactSignature;
	}

	signData(): never {
		throw new Error('KMS Signer does not support sync signing');
	}

	/// Gets the AWS pub key from the AWS KMS service
	/// and converts it to Sui public key format (Secp256k1PublicKey).
	async #getAWSPublicKey() {
		// gets AWS KMS Public Key in DER format
		// returns Sui Public Key

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

		// bitString creates a Uint8ClampedArray
		const rawPublicKey = publicKeyElement.bitString;
		const compressedKey = compressPublicKeyClamped(rawPublicKey);

		return new Secp256k1PublicKey(compressedKey);
	}
}
