// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { fromB64 } from '@mysten/bcs';
import { publicKeyFromRawBytes } from '../verify/index.js';
import type { SignatureFlag } from '../cryptography/index.js';
import { SIGNATURE_FLAG_TO_SCHEME } from '../cryptography/index.js';

export * from './publickey.js';

export function publicKeyFromSuiBytes(publicKey: string | Uint8Array) {
	const bytes = typeof publicKey === 'string' ? fromB64(publicKey) : publicKey;

	const signatureScheme = SIGNATURE_FLAG_TO_SCHEME[bytes[0] as SignatureFlag];

	return publicKeyFromRawBytes(signatureScheme, bytes.slice(1));
}
