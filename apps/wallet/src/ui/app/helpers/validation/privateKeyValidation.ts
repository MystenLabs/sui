// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import { z } from 'zod';

export const privateKeyValidation = z
	.string()
	.trim()
	.nonempty('Private Key is required.')
	.transform((privateKey, context) => {
		try {
			decodeSuiPrivateKey(privateKey);
		} catch (error) {
			context.addIssue({
				code: 'custom',
				message:
					'Invalid Private Key, please use a Bech32 encoded 33-byte string. Learn more: https://github.com/sui-foundation/sips/blob/main/sips/sip-15.md',
			});
			return z.NEVER;
		}
		return privateKey;
	});
