// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { fromB64 } from '@mysten/sui.js/utils';
import mitt from 'mitt';
import { throttle } from 'throttle-debounce';

import {
	type Account,
	isImportedOrDerivedAccount,
	isQredoAccount,
	isLedgerAccount,
} from './Account';
import { DerivedAccount } from './DerivedAccount';
import { ImportedAccount } from './ImportedAccount';
import { LedgerAccount, type SerializedLedgerAccount } from './LedgerAccount';
import { QredoAccount } from './QredoAccount';
import { VaultStorage } from './VaultStorage';
import {
	type AccountsPublicInfoUpdates,
	getStoredAccountsPublicInfo,
	updateAccountsPublicInfo,
} from './accounts';
import { getAllQredoConnections } from '../qredo/storage';
import { getFromLocalStorage, setToLocalStorage } from '../storage-utils';
import { createMessage } from '_messages';
import { isKeyringPayload } from '_payloads/keyring';
import { entropyToSerialized } from '_shared/utils/bip39';
import Alarms from '_src/background/Alarms';
import {
	AUTO_LOCK_TIMER_MAX_MINUTES,
	AUTO_LOCK_TIMER_MIN_MINUTES,
	AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';
import { type Wallet } from '_src/shared/qredo-api';

import type { UiConnection } from '../connections/UiConnection';
import type { ExportedKeypair } from '@mysten/sui.js/cryptography';
import type { Message } from '_messages';
import type { ErrorPayload } from '_payloads';
import type { KeyringPayload } from '_payloads/keyring';

/** The key for the extension's storage, that holds the index of the last derived account (zero based) */
const STORAGE_LAST_ACCOUNT_INDEX_KEY = 'last_account_index';
const STORAGE_ACTIVE_ACCOUNT = 'active_account';

const STORAGE_IMPORTED_LEDGER_ACCOUNTS = 'imported_ledger_accounts';

type KeyringEvents = {
	lockedStatusUpdate: boolean;
	accountsChanged: Account[];
	activeAccountChanged: string;
};

// exported to make testing easier the default export should be used
export class Keyring {
	#events = mitt<KeyringEvents>();
	#locked = true;
	#mainDerivedAccount: string | null = null;
	#accountsMap: Map<string, Account> = new Map();
	public readonly reviveDone: Promise<void>;

	constructor() {
		this.reviveDone = this.revive().catch((e) => {
			// if for some reason decrypting the vault fails or anything else catch
			// the error to allow the user to login using the password
		});
	}

	/**
	 * Creates a vault and stores it encrypted to the storage of the extension. It doesn't unlock the vault.
	 * @param password The password to encrypt the vault
	 * @param importedEntropy The entropy that was generated from an existing mnemonic that the user provided
	 * @throws If the wallet exists or any other error during encrypting/saving to storage or if importedEntropy is invalid
	 */
	public async createVault(password: string, importedEntropy?: string) {
		await VaultStorage.create(password, importedEntropy);
	}

	public async lock() {
		this.#accountsMap.clear();
		this.#mainDerivedAccount = null;
		this.#locked = true;
		await VaultStorage.lock();
		await Alarms.clearLockAlarm();
		this.notifyLockedStatusUpdate(this.#locked);
	}

	public async unlock(password: string) {
		await VaultStorage.unlock(password);
		await this.unlocked();
	}

	public async clearVault() {
		this.lock();
		await VaultStorage.clear();
	}

	public async isWalletInitialized() {
		return await VaultStorage.isWalletInitialized();
	}

	public get isLocked() {
		return this.#locked;
	}

	public on = this.#events.on;

	public off = this.#events.off;

	public async getActiveAccount() {
		if (this.isLocked) {
			return null;
		}
		const address = await getFromLocalStorage(STORAGE_ACTIVE_ACCOUNT, this.#mainDerivedAccount);
		return (
			(address && this.#accountsMap.get(address)) ||
			(this.#mainDerivedAccount && this.#accountsMap.get(this.#mainDerivedAccount)) ||
			null
		);
	}

	public async deriveNextAccount() {
		if (this.isLocked) {
			return null;
		}
		const mnemonicSeedHex = VaultStorage.getMnemonicSeedHex();
		if (!mnemonicSeedHex) {
			return null;
		}
		const nextIndex = (await this.getLastDerivedIndex()) + 1;
		await this.storeLastDerivedIndex(nextIndex);
		const account = this.deriveAccount(nextIndex, mnemonicSeedHex);
		this.#accountsMap.set(account.address, account);
		this.notifyAccountsChanged();
		return account;
	}

	public async importLedgerAccounts(ledgerAccounts: SerializedLedgerAccount[]) {
		if (this.isLocked) {
			return null;
		}

		await this.storeLedgerAccounts(ledgerAccounts);
		const accountsPublicInfoUpdates = [];
		for (const ledgerAccount of ledgerAccounts) {
			const account = new LedgerAccount({
				derivationPath: ledgerAccount.derivationPath,
				address: ledgerAccount.address,
				publicKey: ledgerAccount.publicKey,
			});
			accountsPublicInfoUpdates.push({
				accountAddress: account.address,
				changes: { publicKey: ledgerAccount.publicKey },
			});
			this.#accountsMap.set(ledgerAccount.address, account);
		}
		await updateAccountsPublicInfo(accountsPublicInfoUpdates);
		this.notifyAccountsChanged();
	}

	public getAccounts() {
		if (this.isLocked) {
			return null;
		}
		return Array.from(this.#accountsMap.values());
	}

	public async changeActiveAccount(address: string) {
		if (!this.isLocked && this.#accountsMap.has(address)) {
			await this.storeActiveAccount(address);
			this.#events.emit('activeAccountChanged', address);
			return true;
		}
		return false;
	}

	/**
	 * Exports the keypair for the specified address. Verifies that the password provided is the correct one and only then returns the keypair.
	 * This is useful to be used for exporting the to the UI for the user to backup etc. Getting accounts and keypairs is possible without using
	 * a password by using {@link Keypair.getAccounts} or {@link Keypair.getActiveAccount} or the change events
	 * @param address The sui address to export the keypair
	 * @param password The current password of the vault
	 * @returns null if locked or address not found or the exported keypair
	 * @throws if wrong password is provided
	 */
	public async exportAccountKeypair(address: string, password: string) {
		if (this.isLocked) {
			return null;
		}
		if (await VaultStorage.verifyPassword(password)) {
			const account = this.#accountsMap.get(address);
			if (!account || !isImportedOrDerivedAccount(account)) {
				return null;
			}
			return account.accountKeypair.exportKeypair();
		} else {
			throw new Error('Wrong password');
		}
	}

	public async importAccountKeypair(keypair: ExportedKeypair, password: string) {
		const currentAccounts = this.getAccounts();
		if (this.isLocked || !currentAccounts) {
			// this function is expected to be called from UI when unlocked
			// so this shouldn't happen
			throw new Error('Wallet is locked');
		}
		const passwordCorrect = await VaultStorage.verifyPassword(password);
		if (!passwordCorrect) {
			// we need to make sure that the password is the same with the one of the current vault because we will
			// update the vault and encrypt it to persist the new keypair in storage
			throw new Error('Wrong password');
		}

		const importedOrDerivedAccounts = currentAccounts.filter(isImportedOrDerivedAccount);
		const added = await VaultStorage.importKeypair(keypair, password, importedOrDerivedAccounts);
		if (added) {
			const importedAccount = new ImportedAccount({
				keypair: added,
			});
			this.#accountsMap.set(importedAccount.address, importedAccount);
			await updateAccountsPublicInfo([
				{
					accountAddress: importedAccount.address,
					changes: {
						publicKey: importedAccount.accountKeypair.publicKey.toBase64(),
					},
				},
			]);
			this.notifyAccountsChanged();
		}
		return added;
	}

	public async storeQredoConnection(
		qredoID: string,
		refreshToken: string,
		password: string,
		newAccounts: Wallet[],
	) {
		if (this.isLocked) {
			throw new Error('Wallet is locked');
		}
		await VaultStorage.storeQredoToken(qredoID, refreshToken, password);
		this.#accountsMap.forEach((anAccount) => {
			if (isQredoAccount(anAccount) && anAccount.qredoConnectionID === qredoID) {
				this.#accountsMap.delete(anAccount.address);
			}
		});
		const accountsPublicInfoUpdates: AccountsPublicInfoUpdates = [];
		newAccounts.forEach(({ address, labels, walletID, publicKey }) => {
			const newAccount = new QredoAccount({
				address,
				qredoConnectionID: qredoID,
				qredoWalletID: walletID,
				labels,
				publicKey,
			});
			accountsPublicInfoUpdates.push({
				accountAddress: newAccount.address,
				changes: { publicKey: newAccount.publicKey },
			});
			this.#accountsMap.set(newAccount.address, newAccount);
		});
		await updateAccountsPublicInfo(accountsPublicInfoUpdates);
		this.notifyAccountsChanged();
	}

	public getQredoRefreshToken(qredoID: string) {
		if (this.isLocked) {
			throw new Error('Wallet is locked');
		}
		return VaultStorage.getQredoToken(qredoID);
	}

	public async handleUiMessage(msg: Message, uiConnection: UiConnection) {
		const { id, payload } = msg;
		try {
			if (isKeyringPayload(payload, 'create') && payload.args !== undefined) {
				const { password, importedEntropy } = payload.args;
				await this.createVault(password, importedEntropy);
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'getEntropy')) {
				if (this.#locked) {
					throw new Error('Keyring is locked. Unlock it first.');
				}
				if (!VaultStorage.entropy) {
					throw new Error('Error vault is empty');
				}
				uiConnection.send(
					createMessage<KeyringPayload<'getEntropy'>>(
						{
							type: 'keyring',
							method: 'getEntropy',
							return: entropyToSerialized(VaultStorage.entropy),
						},
						id,
					),
				);
			} else if (isKeyringPayload(payload, 'unlock') && payload.args) {
				await this.unlock(payload.args.password);
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'walletStatusUpdate')) {
				// wait to avoid ui showing locked and then unlocked screen
				// ui waits until it receives this status to render
				await this.reviveDone;
				uiConnection.sendLockedStatusUpdate(this.isLocked, id);
			} else if (isKeyringPayload(payload, 'lock')) {
				this.lock();
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'clear')) {
				await this.clearVault();
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'appStatusUpdate')) {
				const appActive = payload.args?.active;
				if (appActive) {
					this.postponeLock();
				}
			} else if (isKeyringPayload(payload, 'setLockTimeout')) {
				if (payload.args) {
					await this.setLockTimeout(payload.args.timeout);
				}
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'signData')) {
				if (this.#locked) {
					throw new Error('Keyring is locked. Unlock it first.');
				}
				if (!payload.args) {
					throw new Error('Missing parameters.');
				}
				const { data, address } = payload.args;
				const account = this.#accountsMap.get(address);
				if (!account) {
					throw new Error(`Account for address ${address} not found in keyring`);
				}

				if (isImportedOrDerivedAccount(account)) {
					const signature = await account.accountKeypair.sign(fromB64(data));
					uiConnection.send(
						createMessage<KeyringPayload<'signData'>>(
							{
								type: 'keyring',
								method: 'signData',
								return: signature,
							},
							id,
						),
					);
				} else {
					throw new Error(`Unable to sign message for account with type ${account.type}`);
				}
			} else if (isKeyringPayload(payload, 'switchAccount')) {
				if (this.#locked) {
					throw new Error('Keyring is locked. Unlock it first.');
				}
				if (!payload.args) {
					throw new Error('Missing parameters.');
				}
				const { address } = payload.args;
				const changed = await this.changeActiveAccount(address);
				if (!changed) {
					throw new Error(`Failed to change account to ${address}`);
				}
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'deriveNextAccount')) {
				const nextAccount = await this.deriveNextAccount();
				if (!nextAccount) {
					throw new Error('Failed to derive next account');
				}
				await updateAccountsPublicInfo([
					{
						accountAddress: nextAccount.address,
						changes: {
							publicKey: nextAccount.accountKeypair.publicKey.toBase64(),
						},
					},
				]);
				uiConnection.send(
					createMessage<KeyringPayload<'deriveNextAccount'>>(
						{
							type: 'keyring',
							method: 'deriveNextAccount',
							return: { accountAddress: nextAccount.address },
						},
						id,
					),
				);
			} else if (isKeyringPayload(payload, 'importLedgerAccounts') && payload.args) {
				await this.importLedgerAccounts(payload.args.ledgerAccounts);
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'verifyPassword') && payload.args) {
				if (!(await VaultStorage.verifyPassword(payload.args.password))) {
					throw new Error('Wrong password');
				}
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'exportAccount') && payload.args) {
				const keyPair = await this.exportAccountKeypair(
					payload.args.accountAddress,
					payload.args.password,
				);

				if (!keyPair) {
					throw new Error(`Account ${payload.args.accountAddress} not found`);
				}
				uiConnection.send(
					createMessage<KeyringPayload<'exportAccount'>>(
						{
							type: 'keyring',
							method: 'exportAccount',
							return: { keyPair },
						},
						id,
					),
				);
			} else if (isKeyringPayload(payload, 'importPrivateKey') && payload.args) {
				const imported = await this.importAccountKeypair(
					payload.args.keyPair,
					payload.args.password,
				);
				if (!imported) {
					throw new Error('Duplicate account not imported');
				}
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'updateAccountPublicInfo') && payload.args) {
				const { updates } = payload.args;
				await updateAccountsPublicInfo(payload.args.updates);
				const ledgerUpdates: Record<string, string> = {};
				for (const {
					accountAddress,
					changes: { publicKey },
				} of updates) {
					const anAccount = this.#accountsMap.get(accountAddress);
					if (publicKey && anAccount && isLedgerAccount(anAccount) && !anAccount.getPublicKey()) {
						anAccount.setPublicKey(publicKey);
						ledgerUpdates[accountAddress] = publicKey;
					}
				}
				if (Object.keys(ledgerUpdates).length) {
					const allStoredLedgerAccounts = await this.getSavedLedgerAccounts();
					for (let anAccount of allStoredLedgerAccounts) {
						if (!anAccount.publicKey && ledgerUpdates[anAccount.address]) {
							anAccount.publicKey = ledgerUpdates[anAccount.address];
						}
					}
					await this.storeLedgerAccounts(allStoredLedgerAccounts);
					this.notifyAccountsChanged();
				}
				uiConnection.send(createMessage({ type: 'done' }, id));
			}
		} catch (e) {
			uiConnection.send(
				createMessage<ErrorPayload>({ code: -1, error: true, message: (e as Error).message }, id),
			);
		}
	}

	private notifyLockedStatusUpdate(isLocked: boolean) {
		this.#events.emit('lockedStatusUpdate', isLocked);
	}

	private postponeLock = throttle(
		1000,
		async () => {
			if (!this.isLocked) {
				await Alarms.setLockAlarm();
			}
		},
		{ noLeading: false },
	);

	private async setLockTimeout(timeout: number) {
		if (timeout > AUTO_LOCK_TIMER_MAX_MINUTES || timeout < AUTO_LOCK_TIMER_MIN_MINUTES) {
			return;
		}
		await setToLocalStorage(AUTO_LOCK_TIMER_STORAGE_KEY, timeout);
		if (!this.isLocked) {
			await Alarms.setLockAlarm();
		}
	}

	private async revive() {
		const unlocked = await VaultStorage.revive();
		if (unlocked) {
			await this.unlocked();
		}
	}

	private async unlocked() {
		let mnemonicSeedHex = VaultStorage.getMnemonicSeedHex();
		if (!mnemonicSeedHex) {
			return;
		}
		Alarms.setLockAlarm();
		const lastAccountIndex = await this.getLastDerivedIndex();
		for (let i = 0; i <= lastAccountIndex; i++) {
			const account = this.deriveAccount(i, mnemonicSeedHex);
			this.#accountsMap.set(account.address, account);
			if (i === 0) {
				this.#mainDerivedAccount = account.address;
			}
		}
		const savedLedgerAccounts = await this.getSavedLedgerAccounts();
		for (const savedLedgerAccount of savedLedgerAccounts) {
			this.#accountsMap.set(
				savedLedgerAccount.address,
				new LedgerAccount({
					derivationPath: savedLedgerAccount.derivationPath,
					address: savedLedgerAccount.address,
					publicKey: savedLedgerAccount.publicKey || null,
				}),
			);
		}
		for (const aQredoConnection of await getAllQredoConnections()) {
			aQredoConnection.accounts.forEach(({ address, labels, walletID, publicKey }) => {
				const account = new QredoAccount({
					address,
					qredoConnectionID: aQredoConnection.id,
					labels,
					qredoWalletID: walletID,
					publicKey,
				});
				this.#accountsMap.set(account.address, account);
			});
		}
		VaultStorage.getImportedKeys()?.forEach((anImportedKey) => {
			const account = new ImportedAccount({
				keypair: anImportedKey,
			});
			// there is a case where we can import a private key of an account that can be derived from the mnemonic but not yet derived
			// if later we derive it skip overriding the derived account with the imported one (convert the imported as derived in a way)
			if (!this.#accountsMap.has(account.address)) {
				this.#accountsMap.set(account.address, account);
			}
		});
		mnemonicSeedHex = null;
		this.#locked = false;
		this.storeAccountsPublicInfo();
		this.notifyLockedStatusUpdate(this.#locked);
	}

	private deriveAccount(accountIndex: number, mnemonicSeedHex: string) {
		const derivationPath = this.makeDerivationPath(accountIndex);
		const keypair = Ed25519Keypair.deriveKeypairFromSeed(mnemonicSeedHex, derivationPath);
		return new DerivedAccount({ keypair, derivationPath });
	}

	private async getLastDerivedIndex() {
		return (await getFromLocalStorage(STORAGE_LAST_ACCOUNT_INDEX_KEY, 0)) || 0;
	}

	private storeLastDerivedIndex(index: number) {
		return setToLocalStorage(STORAGE_LAST_ACCOUNT_INDEX_KEY, index);
	}

	private storeActiveAccount(address: string) {
		return setToLocalStorage(STORAGE_ACTIVE_ACCOUNT, address);
	}

	private async getSavedLedgerAccounts() {
		const ledgerAccounts = await getFromLocalStorage<SerializedLedgerAccount[]>(
			STORAGE_IMPORTED_LEDGER_ACCOUNTS,
			[],
		);
		return ledgerAccounts || [];
	}

	private storeLedgerAccounts(ledgerAccounts: SerializedLedgerAccount[]) {
		return setToLocalStorage(STORAGE_IMPORTED_LEDGER_ACCOUNTS, ledgerAccounts);
	}

	private makeDerivationPath(index: number) {
		// currently returns only Ed25519 path
		return `m/44'/784'/${index}'/0'/0'`;
	}

	private notifyAccountsChanged() {
		this.#events.emit('accountsChanged', this.getAccounts() || []);
	}

	/**
	 * Do this on first unlock after the account is created to store the public info of the accounts
	 * the first derived if it's a new one (or for accounts that already have been created before this to migrate the storage)
	 */
	private async storeAccountsPublicInfo() {
		if (!Object.keys(await getStoredAccountsPublicInfo()).length) {
			const allAccounts = this.getAccounts();
			if (allAccounts) {
				await updateAccountsPublicInfo(
					allAccounts.map((anAccount) => ({
						accountAddress: anAccount.address,
						changes: { publicKey: anAccount.getPublicKey() },
					})),
				);
			}
		}
	}
}

const keyring = new Keyring();
export default keyring;
