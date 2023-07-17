// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi, beforeEach } from 'vitest';

import { Keyring } from '.';
import { type DerivedAccount } from './DerivedAccount';
import { VaultStorage } from './VaultStorage';
import { getFromLocalStorage, setToLocalStorage } from '../storage-utils';
import Alarm from '_src/background/Alarms';
import {
	testEd25519,
	testEd25519Serialized,
	testMnemonicSeedHex,
	testSecp256k1,
	testSecp256k1Address,
	testSecp256k1Serialized,
} from '_src/test-utils/vault';

import type { MockedObject } from 'vitest';

vi.mock('_src/background/Alarms');
vi.mock('./VaultStorage');
vi.mock('../storage-utils');

describe('Keyring', () => {
	let vaultStorageMock: MockedObject<typeof VaultStorage>;

	beforeEach(() => {
		vaultStorageMock = vi.mocked(VaultStorage);
		vi.mocked(setToLocalStorage).mockResolvedValue();
		vi.mocked(getFromLocalStorage).mockImplementation(async (_, val) => val);
		vi.mocked(Alarm.clearLockAlarm).mockResolvedValue(true);
		vi.mocked(Alarm.setLockAlarm).mockResolvedValue();
	});

	it('initializes and is locked', async () => {
		vaultStorageMock.revive.mockResolvedValue(false);
		const k = new Keyring();
		await k.reviveDone;
		expect(k.isLocked).toBe(true);
	});

	describe('when Keyring is unlocked', () => {
		let k: Keyring;

		beforeEach(async () => {
			vaultStorageMock.revive.mockResolvedValue(true);
			vaultStorageMock.getMnemonicSeedHex.mockReturnValue(testMnemonicSeedHex);
			vaultStorageMock.getImportedKeys.mockReturnValue([testSecp256k1]);
			k = new Keyring();
			await k.reviveDone;
		});

		it('unlocks from session storage', async () => {
			expect(k.isLocked).toBe(false);
		});

		describe('getActiveAccount', () => {
			it('returns as active account the first derived from mnemonic', async () => {
				const account = (await k.getActiveAccount()) as DerivedAccount;
				expect(account.address).toBe(
					'0xf29e2bbf4e0ca0f707b8a4e5213b629f22b1f0e2a1a7056a5b0a7359ac31b97a',
				);
				expect(account.derivationPath).toBe("m/44'/784'/0'/0'/0'");
			});
		});

		describe('deriveNextAccount', () => {
			it('creates the account with index 1 and emits a change event', async () => {
				const eventSpy = vi.fn();
				k.on('accountsChanged', eventSpy);
				const account = await k.deriveNextAccount();
				expect(account!.derivationPath).toBe("m/44'/784'/1'/0'/0'");
				const accounts = k.getAccounts();
				expect(accounts?.length).toBe(3);
				expect(eventSpy).toHaveBeenCalledOnce();
				expect(eventSpy.mock.calls[0][0].length).toBe(3);
			});
		});

		describe('changeActiveAccount', () => {
			it('does not change the active account when not existing address provided', async () => {
				const eventSpy = vi.fn();
				k.on('activeAccountChanged', eventSpy);
				const result = await k.changeActiveAccount('test');
				expect(result).toBe(false);
				expect(eventSpy).not.toHaveBeenCalled();
			});

			it('changes to new account', async () => {
				const eventSpy = vi.fn();
				k.on('activeAccountChanged', eventSpy);
				const result = await k.changeActiveAccount(testSecp256k1Address);
				expect(result).toBe(true);
				expect(eventSpy).toHaveBeenCalledOnce();
				expect(eventSpy).toHaveBeenCalledWith(testSecp256k1Address);
			});
		});

		describe('exportAccountKeypair', () => {
			it('exports the keypair', async () => {
				vaultStorageMock.verifyPassword.mockResolvedValue(true);
				const exportedKeypair = await k.exportAccountKeypair(
					testSecp256k1Address,
					'correct password',
				);
				expect(exportedKeypair).toEqual(testSecp256k1Serialized);
			});

			it('returns null when address not found', async () => {
				vaultStorageMock.verifyPassword.mockResolvedValue(true);
				const exportedKeypair = await k.exportAccountKeypair('unknown', 'correct password');
				expect(exportedKeypair).toBeNull();
			});

			it('throws when wrong password', async () => {
				vaultStorageMock.verifyPassword.mockResolvedValue(false);
				await expect(k.exportAccountKeypair('unknown', 'wrong password')).rejects.toThrow();
			});
		});

		describe('importAccountKeypair', () => {
			it('imports the keypair', async () => {
				const eventSpy = vi.fn();
				k.on('accountsChanged', eventSpy);
				vaultStorageMock.verifyPassword.mockResolvedValue(true);
				vaultStorageMock.importKeypair.mockResolvedValue(testEd25519);
				const added = await k.importAccountKeypair(testEd25519Serialized, 'correct password');
				expect(added).toBeTruthy();
				expect(eventSpy).toHaveBeenCalledOnce();
			});

			it('does not import the keypair if already exists', async () => {
				const eventSpy = vi.fn();
				k.on('accountsChanged', eventSpy);
				vaultStorageMock.verifyPassword.mockResolvedValue(true);
				vaultStorageMock.importKeypair.mockResolvedValue(null);
				const added = await k.importAccountKeypair(testEd25519Serialized, 'correct password');
				expect(added).toBe(null);
				expect(eventSpy).not.toHaveBeenCalled();
			});

			it('throws when wrong password', async () => {
				const eventSpy = vi.fn();
				k.on('accountsChanged', eventSpy);
				vaultStorageMock.verifyPassword.mockResolvedValue(false);
				await expect(
					k.importAccountKeypair(testEd25519Serialized, 'wrong password'),
				).rejects.toThrow();
				expect(eventSpy).not.toHaveBeenCalled();
				expect(vaultStorageMock.importKeypair).not.toHaveBeenCalled();
			});
		});
	});
});
