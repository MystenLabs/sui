// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

import { ImportedAccount } from './ImportedAccount';
import { EPHEMERAL_PASSWORD_KEY, EPHEMERAL_VAULT_KEY, VaultStorage } from './VaultStorage';
import {
	getFromLocalStorage,
	getFromSessionStorage,
	setToLocalStorage,
	setToSessionStorage,
	isSessionStorageSupported,
} from '../storage-utils';
import {
	testEd25519Serialized,
	testDataVault1,
	testDataVault2,
	testEntropySerialized,
	testEd25519,
	testMnemonicSeedHex,
} from '_src/test-utils/vault';

vi.mock('../storage-utils');

describe('VaultStorage', () => {
	beforeEach(() => {
		vi.mocked(isSessionStorageSupported).mockReturnValue(true);
		vi.mocked(getFromLocalStorage).mockResolvedValue(testDataVault1.encrypted.v2);
		vi.mocked(setToLocalStorage).mockResolvedValue();
		vi.mocked(setToSessionStorage).mockResolvedValue();
	});

	afterEach(() => {
		VaultStorage.clear();
	});

	describe('create', () => {
		it('throws if already initialized', async () => {
			await expect(VaultStorage.create('12345')).rejects.toThrow();
		});

		it('uses random entropy when not provided and creates the vault', async () => {
			vi.mocked(getFromLocalStorage).mockResolvedValue(null);
			await VaultStorage.create('12345');
			expect(setToLocalStorage).toHaveBeenCalledOnce();
			expect(setToLocalStorage).toHaveBeenCalledWith('vault', {
				v: 2,
				data: expect.stringContaining('data'),
			});
		});

		it('uses the provided entropy and creates the vault', async () => {
			vi.mocked(getFromLocalStorage).mockResolvedValue(null);
			await VaultStorage.create('12345', testEntropySerialized);
			const storedStore = vi.mocked(setToLocalStorage).mock.calls[0][1];
			vi.mocked(getFromLocalStorage).mockResolvedValue(storedStore);
			await VaultStorage.unlock('12345');
			expect(await VaultStorage.getMnemonicSeedHex()).toBe(testMnemonicSeedHex);
		});
	});

	describe('lock - unlock', () => {
		it('throws if wallet is not initialized', async () => {
			vi.mocked(getFromLocalStorage).mockResolvedValue(null);
			await expect(VaultStorage.unlock('12345')).rejects.toThrow();
		});

		it('throws if password is wrong', async () => {
			await expect(VaultStorage.unlock('wrong password')).rejects.toThrow();
		});

		it('unlocks and locks updating vault session storage', async () => {
			await VaultStorage.unlock(testDataVault1.password);
			expect(VaultStorage.getMnemonicSeedHex()).toBe(testDataVault1.testMnemonicSeedHex);
			expect(setToSessionStorage).toBeCalledTimes(2);
			expect(setToSessionStorage).toHaveBeenNthCalledWith(
				1,
				EPHEMERAL_PASSWORD_KEY,
				expect.stringMatching(/.+/),
			);
			expect(setToSessionStorage).toHaveBeenNthCalledWith(2, EPHEMERAL_VAULT_KEY, {
				v: 2,
				data: expect.stringContaining('data'),
			});
			vi.mocked(setToSessionStorage).mockClear();
			await VaultStorage.lock();
			expect(VaultStorage.getMnemonicSeedHex()).toBe(null);
			expect(setToSessionStorage).toBeCalledTimes(2);
			expect(setToSessionStorage).toHaveBeenNthCalledWith(1, EPHEMERAL_PASSWORD_KEY, null);
			expect(setToSessionStorage).toHaveBeenNthCalledWith(2, EPHEMERAL_VAULT_KEY, null);
		});
	});

	describe('revive', async () => {
		it('unlocks the vault when found in session storage', async () => {
			vi.mocked(getFromSessionStorage).mockImplementation(
				// @ts-expect-error key is string
				(key) => testDataVault1.sessionStorage[key] || null,
			);
			const isUnlocked = await VaultStorage.revive();
			expect(isUnlocked).toBe(true);
			expect(VaultStorage.getMnemonicSeedHex()).toBe(testDataVault1.testMnemonicSeedHex);
		});

		it('keeps vault locked when encrypted vault is not found in session storage', async () => {
			vi.mocked(getFromSessionStorage).mockResolvedValue(null);
			await expect(VaultStorage.revive()).resolves.toBe(false);
			expect(VaultStorage.getMnemonicSeedHex()).toBe(null);
		});
	});

	describe('isWalletInitialized', () => {
		it('returns true when vault is set in storage', async () => {
			expect(await VaultStorage.isWalletInitialized()).toBe(true);
		});

		it('returns false when vault is not set in storage', async () => {
			vi.mocked(getFromLocalStorage).mockResolvedValue(null);
			expect(await VaultStorage.isWalletInitialized()).toBe(false);
		});
	});

	describe('getImportedKeys', () => {
		it('returns null when vault is locked', () => {
			expect(VaultStorage.getImportedKeys()).toBe(null);
		});

		it('it returns the keypairs', async () => {
			await VaultStorage.unlock(testDataVault1.password);
			expect(VaultStorage.getImportedKeys()?.length).toBe(2);
		});
	});

	describe('verifyPassword', () => {
		it('throws when wallet is not initialized', async () => {
			vi.mocked(getFromLocalStorage).mockResolvedValue(null);
			await expect(VaultStorage.verifyPassword('')).rejects.toThrow();
		});

		it('returns true when password is correct', async () => {
			expect(await VaultStorage.verifyPassword(testDataVault1.password)).toBe(true);
		});

		it('returns false when password is wrong', async () => {
			expect(await VaultStorage.verifyPassword('wrong pass')).toBe(false);
		});
	});

	describe('importKeypair', () => {
		it('throws when vault is locked', async () => {
			await expect(
				VaultStorage.importKeypair(testEd25519Serialized, '12345', []),
			).rejects.toThrow();
		});

		it('imports the keypair and saves vault to local and session storage', async () => {
			vi.mocked(getFromLocalStorage).mockResolvedValue(testDataVault2.encrypted.v2);
			await VaultStorage.unlock(testDataVault2.password);
			vi.mocked(setToSessionStorage).mockClear();
			expect(VaultStorage.getImportedKeys()?.length).toBe(0);
			expect(
				await VaultStorage.importKeypair(testEd25519Serialized, testDataVault2.password, []),
			).toBeTruthy();
			expect(VaultStorage.getImportedKeys()?.length).toBe(1);
			expect(setToLocalStorage).toHaveBeenCalledOnce();
			expect(setToSessionStorage).toHaveBeenCalledTimes(2);
			vi.mocked(getFromLocalStorage).mockResolvedValue(
				vi.mocked(setToLocalStorage).mock.calls[0][1],
			);
			vi.mocked(getFromSessionStorage).mockImplementation(
				async (key) =>
					vi.mocked(setToSessionStorage).mock.calls.find((aCall) => aCall[0] === key)?.[1] || null,
			);
			VaultStorage.clear();
			await VaultStorage.unlock(testDataVault2.password);
			expect(VaultStorage.getImportedKeys()?.length).toBe(1);
			VaultStorage.clear();
			await VaultStorage.revive();
			expect(VaultStorage.getImportedKeys()?.length).toBe(1);
		});

		it("it doesn't import existing keys", async () => {
			await VaultStorage.unlock(testDataVault1.password);
			expect(
				await VaultStorage.importKeypair(testEd25519Serialized, testDataVault1.password, [
					new ImportedAccount({
						keypair: testEd25519,
					}),
				]),
			).toBe(null);
		});
	});
});

describe('VaultStorage no session storage', () => {
	beforeEach(() => {
		vi.mocked(getFromLocalStorage).mockResolvedValue(testDataVault1.encrypted.v2);
		vi.mocked(getFromSessionStorage).mockResolvedValue(undefined);
		vi.mocked(isSessionStorageSupported).mockReturnValue(false);
	});

	it('unlocks & locks vault', async () => {
		await VaultStorage.unlock(testDataVault1.password);
		expect(VaultStorage.getMnemonicSeedHex()).toBe(testDataVault1.testMnemonicSeedHex);
		await VaultStorage.lock();
		expect(VaultStorage.getMnemonicSeedHex()).toBe(null);
	});

	it('keeps vault locked when session storage in not defined', async () => {
		await expect(VaultStorage.revive()).resolves.toBe(false);
		expect(VaultStorage.getMnemonicSeedHex()).toBe(null);
	});
});
