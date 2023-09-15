// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { fromB64 } from '@mysten/sui.js/utils';
import mitt from 'mitt';

import { type Account, isImportedOrDerivedAccount, isQredoAccount } from './Account';
import { DerivedAccount } from './DerivedAccount';
import { ImportedAccount } from './ImportedAccount';
import { LedgerAccount, type SerializedLedgerAccount } from './LedgerAccount';
import { QredoAccount } from './QredoAccount';
import { VaultStorage } from './VaultStorage';
import { getAllQredoConnections } from '../qredo/storage';
import { getFromLocalStorage, setToLocalStorage } from '../storage-utils';
import { createMessage } from '_messages';
import { isKeyringPayload } from '_payloads/keyring';
import { type Wallet } from '_src/shared/qredo-api';

import type { UiConnection } from '../connections/UiConnection';
import type { ExportedKeypair } from '@mysten/sui.js/cryptography';
import type { Message } from '_messages';
import type { ErrorPayload } from '_payloads';
import type { KeyringPayload } from '_payloads/keyring';

/** The key for the extension's storage, that holds the index of the last derived account (zero based) */
export const STORAGE_LAST_ACCOUNT_INDEX_KEY = 'last_account_index';

export const STORAGE_IMPORTED_LEDGER_ACCOUNTS = 'imported_ledger_accounts';

type KeyringEvents = {
	lockedStatusUpdate: boolean;
	accountsChanged: Account[];
	activeAccountChanged: string;
};

export async function getSavedLedgerAccounts() {
	const ledgerAccounts = await getFromLocalStorage<SerializedLedgerAccount[]>(
		STORAGE_IMPORTED_LEDGER_ACCOUNTS,
		[],
	);
	return ledgerAccounts || [];
}

/**
 * @deprecated
 */
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
		this.notifyAccountsChanged();
	}

	public getAccounts() {
		if (this.isLocked) {
			return null;
		}
		return Array.from(this.#accountsMap.values());
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
		newAccounts.forEach(({ address, labels, walletID, publicKey, network }) => {
			const newAccount = new QredoAccount({
				address,
				qredoConnectionID: qredoID,
				qredoWalletID: walletID,
				labels,
				publicKey,
				network,
				walletID,
			});
			this.#accountsMap.set(newAccount.address, newAccount);
		});
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
			} else if (isKeyringPayload(payload, 'unlock') && payload.args) {
				await this.unlock(payload.args.password);
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'lock')) {
				this.lock();
				uiConnection.send(createMessage({ type: 'done' }, id));
			} else if (isKeyringPayload(payload, 'clear')) {
				await this.clearVault();
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
			} else if (isKeyringPayload(payload, 'deriveNextAccount')) {
				const nextAccount = await this.deriveNextAccount();
				if (!nextAccount) {
					throw new Error('Failed to derive next account');
				}
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
		const lastAccountIndex = await this.getLastDerivedIndex();
		for (let i = 0; i <= lastAccountIndex; i++) {
			const account = this.deriveAccount(i, mnemonicSeedHex);
			this.#accountsMap.set(account.address, account);
			if (i === 0) {
				this.#mainDerivedAccount = account.address;
			}
		}
		const savedLedgerAccounts = await getSavedLedgerAccounts();
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
			aQredoConnection.accounts.forEach(({ address, labels, walletID, publicKey, network }) => {
				const account = new QredoAccount({
					address,
					qredoConnectionID: aQredoConnection.id,
					labels,
					qredoWalletID: walletID,
					publicKey,
					network,
					walletID,
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
}

const keyring = new Keyring();

/**
 * @deprecated
 */
export default keyring;
