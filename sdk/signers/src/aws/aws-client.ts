// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Secp256k1PublicKey } from '@mysten/sui/keypairs/secp256k1';
import { Secp256r1PublicKey } from '@mysten/sui/keypairs/secp256r1';
import { fromBase64 } from '@mysten/sui/utils';
import { ASN1Construction, ASN1TagClass, DERElement } from 'asn1-ts';

import { AwsClient } from './aws4fetch.js';
import { compressPublicKeyClamped } from './utils.js';

interface KmsCommands {
	Sign: {
		request: {
			KeyId: string;
			Message: string;
			MessageType: 'RAW' | 'DIGEST';
			SigningAlgorithm: 'ECDSA_SHA_256';
		};
		response: {
			KeyId: string;
			KeyOrigin: string;
			Signature: string;
			SigningAlgorithm: string;
		};
	};
	GetPublicKey: {
		request: { KeyId: string };
		response: {
			CustomerMasterKeySpec: string;
			KeyId: string;
			KeyOrigin: string;
			KeySpec: string;
			KeyUsage: string;
			PublicKey: string;
			SigningAlgorithms: string[];
		};
	};
}

export interface AwsClientOptions extends Partial<ConstructorParameters<typeof AwsClient>[0]> {}

export class AwsKmsClient extends AwsClient {
	constructor(options: AwsClientOptions = {}) {
		if (!options.accessKeyId || !options.secretAccessKey) {
			throw new Error('AWS Access Key ID and Secret Access Key are required');
		}

		if (!options.region) {
			throw new Error('Region is required');
		}

		super({
			region: options.region,
			accessKeyId: options.accessKeyId,
			secretAccessKey: options.secretAccessKey,
			service: 'kms',
			...options,
		});
	}

	async getPublicKey(keyId: string) {
		const publicKeyResponse = await this.runCommand('GetPublicKey', { KeyId: keyId });

		if (!publicKeyResponse.PublicKey) {
			throw new Error('Public Key not found for the supplied `keyId`');
		}

		const publicKey = fromBase64(publicKeyResponse.PublicKey);

		const encodedData: Uint8Array = publicKey;
		const derElement = new DERElement();
		derElement.fromBytes(encodedData);

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

		const compressedKey = compressPublicKeyClamped(publicKeyElement.bitString);

		switch (publicKeyResponse.KeySpec) {
			case 'ECC_NIST_P256':
				return new Secp256r1PublicKey(compressedKey);
			case 'ECC_SECG_P256K1':
				return new Secp256k1PublicKey(compressedKey);
			default:
				throw new Error('Unsupported key spec: ' + publicKeyResponse.KeySpec);
		}
	}

	async runCommand<T extends keyof KmsCommands>(
		command: T,
		body: KmsCommands[T]['request'],
		{
			region = this.region!,
		}: {
			region?: string;
		} = {},
	): Promise<KmsCommands[T]['response']> {
		if (!region) {
			throw new Error('Region is required');
		}

		const res = await this.fetch(`https://kms.${region}.amazonaws.com/`, {
			headers: {
				'Content-Type': 'application/x-amz-json-1.1',
				'X-Amz-Target': `TrentService.${command}`,
			},
			body: JSON.stringify(body),
		});

		if (!res.ok) {
			throw new Error(await res.text());
		}

		return res.json();
	}
}
