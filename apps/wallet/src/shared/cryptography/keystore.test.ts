// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';

import { decrypt, encrypt } from './keystore';

describe('keystore', () => {
	it('encrypt and decrypt success', async () => {
		const password = 'password';
		const plaintext = JSON.stringify('hello world');
		const ciphertext = await encrypt(password, plaintext);
		const result = await decrypt<string>(password, ciphertext);
		expect(result).toBe(plaintext);
	});

	it('encrypt and decrypt failed with wrong password', async () => {
		const password = 'password';
		const plaintext = JSON.stringify('hello world');
		const ciphertext = await encrypt(password, plaintext);
		await expect(decrypt('random', ciphertext)).rejects.toThrow('Incorrect password');
	});
});
