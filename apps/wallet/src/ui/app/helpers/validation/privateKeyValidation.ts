// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decodeSuiPrivateKey } from '@mysten/sui.js/cryptography/keypair';
import { hexToBytes } from '@noble/hashes/utils';
import * as Yup from 'yup';
import { z } from 'zod';

export const privateKeyValidation = z
	.string()
	.trim()
	.nonempty('Private Key is a required field.')
	.transform((privateKey, context) => {
		try {
			decodeSuiPrivateKey(privateKey);
		} catch (error) {
			context.addIssue({
				code: 'custom',
				message: 'Private Key must be a Bech32 encoded 33-byte string',
			});
			return z.NEVER;
		}
		return privateKey;
	});

/** @deprecated Prefer Zod over Yup for doing schema validation! */
export const deprecatedPrivateKeyValidation = Yup.string()
	.ensure()
	.trim()
	.required()
	.transform((value: string) => {
		if (value.startsWith('0x')) {
			return value.substring(2);
		}
		return value;
	})
	.test(
		'valid-hex',
		`\${path} must be a hexadecimal value. It may optionally begin with "0x".`,
		(value: string) => {
			try {
				hexToBytes(value);
				return true;
			} catch (e) {
				return false;
			}
		},
	)
	.test('valid-bytes-length', `\${path} must be either 32 or 64 bytes.`, (value: string) => {
		try {
			const bytes = hexToBytes(value);
			return [32, 64].includes(bytes.length);
		} catch (e) {
			return false;
		}
	})
	.label('Private Key');
