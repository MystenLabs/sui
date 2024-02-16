// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decodeSuiPrivateKey, encodeSuiPrivateKey } from '@mysten/sui.js/cryptography/keypair';
import { hexToBytes } from '@noble/hashes/utils';
import { z } from 'zod';

export const privateKeyValidation = z
	.string()
	.trim()
	.nonempty('Private Key is required.')
	.transform((privateKey, context) => {
		if (!privateKey.startsWith('suiprivkey')) {
			const hexValue = privateKey.startsWith('0x') ? privateKey.slice(2) : privateKey;
			let privateKeyBytes: Uint8Array | undefined;

			try {
				privateKeyBytes = hexToBytes(hexValue);
			} catch (error) {
				context.addIssue({
					code: 'custom',
					message: 'Invalid Private Key, please use a Bech32 encoded 33-byte string.',
				});
				return z.NEVER;
			}

			if (![32, 64].includes(privateKeyBytes.length)) {
				context.addIssue({
					code: 'custom',
					message: 'Hex encoded Private Key must be either 32 or 64 bytes.',
				});
				return z.NEVER;
			}

			return encodeSuiPrivateKey(privateKeyBytes.slice(0, 32), 'ED25519');
		}
		try {
			decodeSuiPrivateKey(privateKey);
		} catch (error) {
			context.addIssue({
				code: 'custom',
				message: 'Invalid Private Key, please use a Bech32 encoded 33-byte string',
			});
			return z.NEVER;
		}
		return privateKey;
	});
