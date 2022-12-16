// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi } from 'vitest';
import Browser from 'webextension-polyfill';

import { VaultStorage } from './VaultStorage';
import { SESSION_STORAGE } from './storage-utils';
import * as Bip39 from '_shared/utils/bip39';
import {
    testEd25519Serialized,
    testVault,
    testVault1,
} from '_src/test-utils/vault';

describe('VaultStorage', () => {
    describe('create', () => {
        it('throws if already initialized', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: {},
            });
            await expect(vaultStorage.create('12345')).rejects.toThrow(
                'Mnemonic already exists, creating a new one will override it. Clear the existing one first.'
            );
        });

        it('uses random entropy when not provided and creates the vault', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({});
            const storageSet = vi
                .spyOn(Browser.storage.local, 'set')
                .mockResolvedValue();
            const getRandomEntropySpy = vi
                .spyOn(Bip39, 'getRandomEntropy')
                .mockReturnValue(new Uint8Array(32));
            await vaultStorage.create('12345');
            expect(storageSet).toHaveBeenCalledOnce();
            expect(storageSet.mock.calls[0][0]).toMatchObject({
                vault: {
                    v: 2,
                    data: expect.stringContaining('data'),
                },
            });
            expect(getRandomEntropySpy).toHaveBeenCalledOnce();
        });

        it('uses the provided entropy and creates the vault', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({});
            const storageSet = vi
                .spyOn(Browser.storage.local, 'set')
                .mockResolvedValue();
            const getRandomEntropySpy = vi
                .spyOn(Bip39, 'getRandomEntropy')
                .mockReturnValue(new Uint8Array(32));
            await vaultStorage.create(
                '12345',
                '842a27e29319123892f9ba8d9991c525'
            );
            expect(storageSet).toHaveBeenCalledOnce();
            expect(storageSet.mock.calls[0][0]).toMatchObject({
                vault: {
                    v: 2,
                    data: expect.stringContaining('data'),
                },
            });
            expect(getRandomEntropySpy).not.toHaveBeenCalled();
        });
    });

    describe('unlock', () => {
        it('throws if wallet is not initialized', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({});
            await expect(vaultStorage.unlock('12345')).rejects.toThrow(
                'Wallet is not initialized. Create a new one first.'
            );
        });

        it('throws if password is wrong', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            await expect(vaultStorage.unlock('wrong password')).rejects.toThrow(
                'Incorrect password'
            );
        });

        it('unlocks the vault', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            await vaultStorage.unlock(testVault.password);
            expect(vaultStorage.getMnemonic()).toBe(testVault.mnemonic);
        });

        it('unlocks the vault and updates session storage', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault1.encrypted.v2,
            });
            await vaultStorage.unlock(testVault.password);
            expect(vaultStorage.getMnemonic()).toBe(testVault.mnemonic);
            const sessionStorageSet = vi.mocked(SESSION_STORAGE!.set);
            expect(sessionStorageSet).toBeCalledTimes(2);
            expect(sessionStorageSet.mock.calls[0][0]).toMatchObject({
                '244e4b24e667ebf': expect.stringMatching(/.+/),
            });
            expect(sessionStorageSet.mock.calls[1][0]).toMatchObject({
                a8e451b8ae8a1b4: {
                    v: 2,
                    data: expect.stringContaining('data'),
                },
            });
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

        it('clears session storage', async () => {
            const vaultStorage = new VaultStorage();
            await vaultStorage.lock();
            const sessionStorageSet = vi.mocked(SESSION_STORAGE!.set);
            expect(sessionStorageSet).toHaveBeenCalledTimes(2);
            expect(sessionStorageSet).toHaveBeenNthCalledWith(1, {
                '244e4b24e667ebf': null,
            });
            expect(sessionStorageSet).toHaveBeenNthCalledWith(2, {
                a8e451b8ae8a1b4: null,
            });
        });
    });

    describe('revive', async () => {
        it('unlocks the vault when found in session storage', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(SESSION_STORAGE!, 'get').mockResolvedValue(
                testVault.sessionStorage
            );
            const isUnlocked = await vaultStorage.revive();
            expect(isUnlocked).toBe(true);
            expect(vaultStorage.getMnemonic()).toBe(testVault.mnemonic);
        });

        it('keeps vault locked when encrypted vault is found in session storage', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(SESSION_STORAGE!, 'get').mockResolvedValue({});
            await expect(vaultStorage.revive()).resolves.toBe(false);
            expect(vaultStorage.getMnemonic()).toBe(null);
        });
    });

    describe('clear', async () => {
        it('locks the vault', async () => {
            const vaultStorage = new VaultStorage();
            const lockSpy = vi.spyOn(vaultStorage, 'lock').mockResolvedValue();
            await vaultStorage.clear();
            expect(lockSpy).toHaveBeenCalledOnce();
        });

        it('clears vault from local storage', async () => {
            const vaultStorage = new VaultStorage();
            const storageSet = vi
                .spyOn(Browser.storage.local, 'set')
                .mockResolvedValue();
            await vaultStorage.clear();
            expect(storageSet).toHaveBeenCalledOnce();
            expect(storageSet).toHaveBeenCalledWith({ vault: null });
        });
    });

    describe('isWalletInitialized', () => {
        it('returns true when vault is set in storage', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: {},
            });
            expect(await vaultStorage.isWalletInitialized()).toBe(true);
        });

        it('returns false when vault is not set in storage', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: null,
            });
            expect(await vaultStorage.isWalletInitialized()).toBe(false);
        });
    });

    describe('getImportedKeys', () => {
        it('returns null when vault is locked', () => {
            const vaultStorage = new VaultStorage();
            expect(vaultStorage.getImportedKeys()).toBe(null);
        });

        it('it returns the keypairs', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            await vaultStorage.unlock(testVault.password);
            expect(vaultStorage.getImportedKeys()?.length).toBe(2);
        });
    });

    describe('verifyPassword', () => {
        it('throws when wallet is not initialized', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: null,
            });
            await expect(vaultStorage.verifyPassword('')).rejects.toThrow(
                'Wallet is not initialized'
            );
        });

        it('returns true when password is correct', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            expect(await vaultStorage.verifyPassword(testVault.password)).toBe(
                true
            );
        });

        it('returns false when password is wrong', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            expect(await vaultStorage.verifyPassword('wrong pass')).toBe(false);
        });
    });

    describe('importKeypair', () => {
        it('throws when vault is locked', async () => {
            const vaultStorage = new VaultStorage();
            await expect(
                vaultStorage.importKeypair(testEd25519Serialized, '12345')
            ).rejects.toThrow(
                'Error, vault is locked. Unlock the vault first.'
            );
        });

        it('imports the keypair and saves vault to local and session storage', async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault1.encrypted.v2,
            });
            await vaultStorage.unlock(testVault1.password);
            const localStorageSpy = vi
                .spyOn(Browser.storage.local, 'set')
                .mockResolvedValue();
            const sessionStorageSpy = vi
                .spyOn(SESSION_STORAGE!, 'set')
                .mockResolvedValue();
            expect(vaultStorage.getImportedKeys()?.length).toBe(0);
            expect(
                await vaultStorage.importKeypair(
                    testEd25519Serialized,
                    testVault1.password
                )
            ).toBe(true);
            expect(vaultStorage.getImportedKeys()?.length).toBe(1);
            expect(localStorageSpy).toHaveBeenCalledOnce();
            expect(sessionStorageSpy).toHaveBeenCalledTimes(2);
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue(
                localStorageSpy.mock.calls[0][0]
            );
            vi.spyOn(SESSION_STORAGE!, 'get').mockResolvedValue({
                ...sessionStorageSpy.mock.calls[0][0],
                ...sessionStorageSpy.mock.calls[1][0],
            });
            const vaultStorage2 = new VaultStorage();
            await vaultStorage2.unlock(testVault1.password);
            expect(vaultStorage2.getImportedKeys()?.length).toBe(1);
            const vaultStorage3 = new VaultStorage();
            await vaultStorage3.revive();
            expect(vaultStorage3.getImportedKeys()?.length).toBe(1);
        });

        it("it doesn't import existing keys", async () => {
            const vaultStorage = new VaultStorage();
            vi.spyOn(Browser.storage.local, 'get').mockResolvedValue({
                vault: testVault.encrypted.v2,
            });
            await vaultStorage.unlock(testVault.password);
            const localStorageSpy = vi
                .spyOn(Browser.storage.local, 'set')
                .mockResolvedValue();
            const sessionStorageSpy = vi
                .spyOn(SESSION_STORAGE!, 'set')
                .mockResolvedValue();
            expect(
                await vaultStorage.importKeypair(
                    testEd25519Serialized,
                    testVault.password
                )
            ).toBe(false);
            expect(localStorageSpy).not.toHaveBeenCalled();
            expect(sessionStorageSpy).not.toHaveBeenCalled();
        });
    });
});
