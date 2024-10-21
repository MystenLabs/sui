// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64, toBase64 } from '@mysten/sui/utils';

/**
 * An interface
 */
export interface Encryption {
	encrypt(password: string, data: string): Promise<string>;
	decrypt(password: string, data: string): Promise<string>;
}

/**
 * Create the default encryption interface, which uses the browsers built-in crypto primitives.
 */
export function createDefaultEncryption(): Encryption {
	type EncryptedJSON = {
		payload: string;
		iv: string;
		salt: string;
	};

	async function keyFromPassword(password: string, salt: Uint8Array) {
		const key = await crypto.subtle.importKey(
			'raw',
			new TextEncoder().encode(password),
			{ name: 'PBKDF2' },
			false,
			['deriveBits', 'deriveKey'],
		);

		const derivedKey = await crypto.subtle.deriveKey(
			{
				name: 'PBKDF2',
				salt,
				iterations: 900_000,
				hash: 'SHA-256',
			},
			key,
			{ name: 'AES-GCM', length: 256 },
			false,
			['encrypt', 'decrypt'],
		);

		return { key, derivedKey };
	}

	return {
		async encrypt(password, data) {
			const salt = crypto.getRandomValues(new Uint8Array(16));
			const iv = crypto.getRandomValues(new Uint8Array(12));

			const { derivedKey } = await keyFromPassword(password, salt);

			const payload = await crypto.subtle.encrypt(
				{
					name: 'AES-GCM',
					iv,
				},
				derivedKey,
				new TextEncoder().encode(data),
			);

			return JSON.stringify({
				payload: toBase64(new Uint8Array(payload)),
				iv: toBase64(iv),
				salt: toBase64(salt),
			} satisfies EncryptedJSON);
		},
		async decrypt(password, data) {
			const parsed = JSON.parse(data) as EncryptedJSON;
			if (!parsed.payload || !parsed.iv || !parsed.salt) {
				throw new Error('Invalid encrypted data');
			}

			const { derivedKey } = await keyFromPassword(password, fromBase64(parsed.salt));

			const decryptedContent = await crypto.subtle.decrypt(
				{
					name: 'AES-GCM',
					iv: fromBase64(parsed.iv),
				},
				derivedKey,
				fromBase64(parsed.payload),
			);

			return new TextDecoder().decode(decryptedContent);
		},
	};
}

/**
 * Create a passthrough encryption interface, which does not encrypt or decrypt data.
 */
export function createPassthroughEncryption(): Encryption {
	return {
		async encrypt(_password, data) {
			return data;
		},
		async decrypt(_password, data) {
			return data;
		},
	};
}
