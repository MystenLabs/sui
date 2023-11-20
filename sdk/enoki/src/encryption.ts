// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	decrypt as metamaskDecrypt,
	encrypt as metamaskEncrypt,
} from '@metamask/browser-passworder';

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
	return {
		async encrypt(password, data) {
			return metamaskEncrypt(password, data);
		},
		async decrypt(password, data) {
			const decrypted = await metamaskDecrypt(password, data);
			return decrypted as string;
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
