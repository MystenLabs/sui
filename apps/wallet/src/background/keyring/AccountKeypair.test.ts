// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RawSigner } from '@mysten/sui.js';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { Secp256k1Keypair } from '@mysten/sui.js/keypairs/secp256k1';
import { describe, expect, it } from 'vitest';

import { AccountKeypair } from './AccountKeypair';

describe('AccountKeypair', () => {
	describe('signData produces same signature with RawSigner', () => {
		it.each([
			{ label: 'Ed25519Keypair', keypair: new Ed25519Keypair() },
			{ label: 'Secp256k1Keypair', keypair: new Secp256k1Keypair() },
		])('for $label', async ({ label, keypair }) => {
			const signData = new TextEncoder().encode('hello world');
			const account = new AccountKeypair(keypair);
			const accountKeypairSignature = await account.sign(signData);
			const rawSigner = new RawSigner(keypair, new SuiClient({ url: getFullnodeUrl('localnet') }));
			const rawSignerSignature = await rawSigner.signData(signData);
			expect(accountKeypairSignature).toBe(rawSignerSignature);
		});
	});
});
