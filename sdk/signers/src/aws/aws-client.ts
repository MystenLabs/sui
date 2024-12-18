// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Secp256k1PublicKey } from '@mysten/sui/keypairs/secp256k1';
import { Secp256r1PublicKey } from '@mysten/sui/keypairs/secp256r1';
import { fromBase64 } from '@mysten/sui/utils';

import { publicKeyFromDER } from '../utils/utils.js';
import { AwsClient } from './aws4fetch.js';

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

		const compressedKey = publicKeyFromDER(fromBase64(publicKeyResponse.PublicKey));

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
