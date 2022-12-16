// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi } from 'vitest';

import { Vault } from './Vault';
import { testVault } from '_src/test-utils/vault';

describe('Vault', () => {
    it('initializes', () => {
        const vault = new Vault(testVault.entropy, [...testVault.keypairs]);
        expect(vault).toBeDefined();
    });

    it('returns the correct mnemonic', () => {
        const vault = new Vault(testVault.entropy, [...testVault.keypairs]);
        expect(vault.getMnemonic()).toBe(testVault.mnemonic);
    });

    it('encrypts itself', async () => {
        const vault = new Vault(testVault.entropy, [...testVault.keypairs]);
        const encryptedVault = await vault.encrypt('a password');
        expect(encryptedVault).toMatchObject({
            v: 2,
        });
    });

    describe.each([
        {
            v: 0,
            storedData: testVault.encrypted.v0,
            triggerMigration: true,
            keypairs: [],
        },
        {
            v: 1,
            storedData: testVault.encrypted.v1,
            triggerMigration: true,
            keypairs: [],
        },
        {
            v: 2,
            storedData: testVault.encrypted.v2,
            triggerMigration: false,
            keypairs: [...testVault.keypairs],
        },
    ])(
        'from v$v encrypted data',
        ({ storedData, keypairs, triggerMigration }) => {
            it('initializes', async () => {
                const vault = await Vault.from(testVault.password, storedData);
                expect(vault.getMnemonic()).toBe(testVault.mnemonic);
                expect(vault.entropy).toEqual(testVault.entropy);
                expect(vault.importedKeypairs).toMatchObject(keypairs);
            });

            it(`${
                triggerMigration ? 'Triggers' : 'Does not trigger'
            } migration callback`, async () => {
                const migrateFN = vi.fn();
                await Vault.from(testVault.password, storedData, migrateFN);
                expect(migrateFN).toBeCalledTimes(triggerMigration ? 1 : 0);
            });
        }
    );
});
