// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi } from 'vitest';
import Browser from 'webextension-polyfill';

import { VaultStorage } from './VaultStorage';
import { SESSION_STORAGE } from './storage-utils';
import { testVault } from '_src/test-utils/vault';

vi.mock('./storage-utils', () => ({ SESSION_STORAGE: null }));

describe('VaultStorage no session storage', () => {
    it('session storage is null', () => {
        expect(SESSION_STORAGE).toBe(null);
    });

    describe('unlock', () => {
        it('unlocks the vault', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            await vaultStorage.unlock(testVault.password);
            expect(vaultStorage.getMnemonic()).toBe(testVault.mnemonic);
        });
    });

    describe('lock', () => {
        it('clears vault', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            await vaultStorage.unlock(testVault.password);
            expect(vaultStorage.getMnemonic()).toBe(testVault.mnemonic);
            await vaultStorage.lock();
            expect(vaultStorage.getMnemonic()).toBe(null);
        });
    });

    describe('revive', async () => {
        it('keeps vault locked when session storage in not defined', async () => {
            const vaultStorage = new VaultStorage();
            await expect(vaultStorage.revive()).resolves.toBe(false);
            expect(vaultStorage.getMnemonic()).toBe(null);
        });
    });
});
