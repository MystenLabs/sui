// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi, beforeEach } from 'vitest';
import Browser from 'webextension-polyfill';

import { Keyring } from '.';
import { VaultStorage } from './VaultStorage';
import Alarm from '_src/background/Alarms';
import {
    testEd25519,
    testEd25519Address,
    testEd25519Serialized,
    testMnemonic,
} from '_src/test-utils/vault';

import type { MockedObject } from 'vitest';

vi.mock('_src/background/Alarms');
vi.mock('./VaultStorage', () => {
    const v = vi.fn();
    v.prototype.revive = vi.fn();
    v.prototype.getMnemonic = vi.fn();
    v.prototype.getImportedKeys = vi.fn();
    v.prototype.verifyPassword = vi.fn();
    v.prototype.importKeypair = vi.fn();
    return { VaultStorage: v };
});

describe('Keyring', () => {
    let vaultStorageMock: MockedObject<VaultStorage>;
    beforeEach(() => {
        vaultStorageMock = vi.mocked(new VaultStorage());
        vi.mocked(Alarm.clearLockAlarm).mockResolvedValue(true);
        vi.mocked(Alarm.setLockAlarm).mockResolvedValue();
    });

    it('initializes and is locked', async () => {
        vaultStorageMock.revive.mockResolvedValue(false);
        const k = new Keyring();
        expect(k).toBeDefined();
        await k.reviveDone;
        expect(k.isLocked).toBe(true);
    });

    it('initializes and unlocks from session storage', async () => {
        vaultStorageMock.revive.mockResolvedValue(true);
        vaultStorageMock.getMnemonic.mockReturnValue(testMnemonic);
        vaultStorageMock.getImportedKeys.mockReturnValue(null);
        vi.spyOn(Browser.storage.local, 'get').mockImplementation(
            async (val) => val as Record<string, unknown>
        );
        const k = new Keyring();
        expect(k).toBeDefined();
        await k.reviveDone;
        expect(k.isLocked).toBe(false);
    });

    describe('when Keyring is unlocked', () => {
        let k: Keyring;

        beforeEach(async () => {
            vaultStorageMock.revive.mockResolvedValue(true);
            vaultStorageMock.getMnemonic.mockReturnValue(testMnemonic);
            vaultStorageMock.getImportedKeys.mockReturnValue([testEd25519]);
            vi.spyOn(Browser.storage.local, 'get').mockImplementation(
                async (val) => val as Record<string, unknown>
            );
            k = new Keyring();
            await k.reviveDone;
        });

        describe('getActiveAccount', () => {
            it('returns as active account the first derived from mnemonic', async () => {
                expect(await k.getActiveAccount()).toBeDefined();
                expect((await k.getActiveAccount())!.address).toBe(
                    '9c08076187d961f1ed809a9d803fa49037a92039'
                );
                expect((await k.getActiveAccount())!.derivationPath).toBe(
                    "m/44'/784'/0'/0'/0'"
                );
            });
        });

        describe('deriveNextAccount', () => {
            it('creates the account with index 1 and emits a change event', async () => {
                const eventSpy = vi.fn();
                k.on('accountsChanged', eventSpy);
                const setSpy = vi
                    .spyOn(Browser.storage.local, 'set')
                    .mockResolvedValue();
                const result = await k.deriveNextAccount();
                expect(result).toBe(true);
                expect(setSpy).toHaveBeenCalledOnce();
                expect(setSpy).toHaveBeenCalledWith({ last_account_index: 1 });
                const accounts = k.getAccounts();
                expect(accounts?.length).toBe(3);
                expect(
                    accounts?.find(
                        (anAccount) =>
                            anAccount.derivationPath === "m/44'/784'/1'/0'/0'"
                    )
                ).toBeTruthy();
                expect(eventSpy).toHaveBeenCalledOnce();
                expect(eventSpy.mock.calls[0][0].length).toBe(3);
            });
        });

        describe('changeActiveAccount', () => {
            it('does not change the active account when not existing address provided', async () => {
                const eventSpy = vi.fn();
                k.on('activeAccountChanged', eventSpy);
                const setSpy = vi
                    .spyOn(Browser.storage.local, 'set')
                    .mockResolvedValue();
                const result = await k.changeActiveAccount('test');
                expect(result).toBe(false);
                expect(setSpy).not.toHaveBeenCalled();
                expect(eventSpy).not.toHaveBeenCalled();
            });

            it('changes to new account', async () => {
                const eventSpy = vi.fn();
                k.on('activeAccountChanged', eventSpy);
                const setSpy = vi
                    .spyOn(Browser.storage.local, 'set')
                    .mockResolvedValue();
                const result = await k.changeActiveAccount(testEd25519Address);
                expect(result).toBe(true);
                expect(setSpy).toHaveBeenCalledOnce();
                expect(setSpy).toHaveBeenCalledWith({
                    active_account: testEd25519Address,
                });
                expect(eventSpy).toHaveBeenCalledOnce();
                expect(eventSpy).toHaveBeenCalledWith(testEd25519Address);
            });
        });

        describe('exportAccountKeypair', () => {
            it('exports the keypair', async () => {
                vaultStorageMock.verifyPassword.mockResolvedValue(true);
                const exportedKeypair = await k.exportAccountKeypair(
                    testEd25519Address,
                    'correct password'
                );
                expect(exportedKeypair).toEqual(testEd25519Serialized);
            });

            it('returns null when address not found', async () => {
                vaultStorageMock.verifyPassword.mockResolvedValue(true);
                const exportedKeypair = await k.exportAccountKeypair(
                    'unknown',
                    'correct password'
                );
                expect(exportedKeypair).toBeNull();
            });

            it('throws when wrong password', async () => {
                vaultStorageMock.verifyPassword.mockResolvedValue(false);
                await expect(
                    k.exportAccountKeypair('unknown', 'wrong password')
                ).rejects.toThrow('Wrong password');
            });
        });

        describe('importAccountKeypair', () => {
            it('imports the keypair', async () => {
                const eventSpy = vi.fn();
                k.on('accountsChanged', eventSpy);
                vaultStorageMock.verifyPassword.mockResolvedValue(true);
                vaultStorageMock.importKeypair.mockResolvedValue(true);
                const added = await k.importAccountKeypair(
                    testEd25519Serialized,
                    'correct password'
                );
                expect(added).toBe(true);
                expect(eventSpy).toHaveBeenCalledOnce();
            });

            it('does not import the keypair if already exists', async () => {
                const eventSpy = vi.fn();
                k.on('accountsChanged', eventSpy);
                vaultStorageMock.verifyPassword.mockResolvedValue(true);
                vaultStorageMock.importKeypair.mockResolvedValue(false);
                const added = await k.importAccountKeypair(
                    testEd25519Serialized,
                    'correct password'
                );
                expect(added).toBe(false);
                expect(eventSpy).not.toHaveBeenCalled();
            });

            it('throws when wrong password', async () => {
                const eventSpy = vi.fn();
                k.on('accountsChanged', eventSpy);
                vaultStorageMock.verifyPassword.mockResolvedValue(false);
                await expect(
                    k.importAccountKeypair(
                        testEd25519Serialized,
                        'wrong password'
                    )
                ).rejects.toThrow('Wrong password');
                expect(eventSpy).not.toHaveBeenCalled();
                expect(vaultStorageMock.importKeypair).not.toHaveBeenCalled();
            });
        });
    });
});
