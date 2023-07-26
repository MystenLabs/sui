// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { hexToBytes } from '@noble/hashes/utils';
import * as Yup from 'yup';

export const privateKeyValidation = Yup.string()
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
