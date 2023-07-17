// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi } from 'vitest';

import { Vault } from './Vault';
import { testDataVault1 } from '_src/test-utils/vault';

describe('Vault', () => {
	it('returns the correct mnemonic', () => {
		const vault = new Vault(testDataVault1.entropy, [...testDataVault1.keypairs]);
		expect(vault.getMnemonic()).toBe(testDataVault1.mnemonic);
	});

	it('encrypts itself', async () => {
		const vault = new Vault(testDataVault1.entropy, [...testDataVault1.keypairs]);
		const encryptedVault = await vault.encrypt('a password');
		expect(encryptedVault).toMatchObject({
			v: 2,
		});
	});

	describe.each([
		{
			v: 0,
			storedData: testDataVault1.encrypted.v0,
			triggerMigration: true,
			keypairs: [],
		},
		{
			v: 1,
			storedData: testDataVault1.encrypted.v1,
			triggerMigration: true,
			keypairs: [],
		},
		{
			v: 2,
			storedData: testDataVault1.encrypted.v2,
			triggerMigration: false,
			keypairs: [...testDataVault1.keypairs],
		},
	])('from v$v encrypted data', ({ storedData, keypairs, triggerMigration }) => {
		it('initializes', async () => {
			const vault = await Vault.from(testDataVault1.password, storedData);
			expect(vault.getMnemonic()).toBe(testDataVault1.mnemonic);
			expect(vault.entropy).toEqual(testDataVault1.entropy);
			expect(vault.importedKeypairs).toMatchObject(keypairs);
		});

		it(`${triggerMigration ? 'Triggers' : 'Does not trigger'} migration callback`, async () => {
			const migrateFN = vi.fn();
			await Vault.from(testDataVault1.password, storedData, migrateFN);
			expect(migrateFN).toBeCalledTimes(triggerMigration ? 1 : 0);
		});
	});
});
