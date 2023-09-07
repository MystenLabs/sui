// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Dexie from 'dexie';
import {
	MnemonicAccountSource,
	deriveKeypairFromSeed,
	makeDerivationPath,
} from './account-sources/MnemonicAccountSource';
import { QredoAccountSource } from './account-sources/QredoAccountSource';
import { accountSourcesEvents } from './account-sources/events';
import { type SerializedAccount } from './accounts/Account';
import { ImportedAccount } from './accounts/ImportedAccount';
import { LedgerAccount } from './accounts/LedgerAccount';
import { MnemonicAccount } from './accounts/MnemonicAccount';
import { type QredoSerializedAccount } from './accounts/QredoAccount';
import { accountsEvents } from './accounts/events';
import { backupDB, getDB } from './db';
import { STORAGE_LAST_ACCOUNT_INDEX_KEY, getSavedLedgerAccounts } from './keyring';
import { VaultStorage } from './keyring/VaultStorage';
import { getAllQredoConnections } from './qredo/storage';
import { getFromLocalStorage, makeUniqueKey, setToLocalStorage } from './storage-utils';

export type Status = 'required' | 'inProgress' | 'ready';

const migrationDoneStorageKey = 'storage-migration-done';
const storageActiveAccountKey = 'active_account';

function getActiveAccountAddress() {
	return getFromLocalStorage<string>(storageActiveAccountKey);
}

let statusCache: Status | null = null;

export async function getStatus() {
	if (statusCache) {
		return statusCache;
	}
	const vaultInitialized = await VaultStorage.isWalletInitialized();
	if (!vaultInitialized) {
		return (statusCache = 'ready');
	}
	const isMigrationDone = await getFromLocalStorage<boolean>(migrationDoneStorageKey);
	if (isMigrationDone) {
		return (statusCache = 'ready');
	}
	return (statusCache = 'required');
}

export function clearStatus() {
	statusCache = null;
}

export async function makeMnemonicAccounts(password: string) {
	if (!VaultStorage.mnemonicSeedHex || !VaultStorage.entropy) {
		throw new Error('Missing mnemonic entropy');
	}
	const currentMnemonicIndex =
		(await getFromLocalStorage<number>(STORAGE_LAST_ACCOUNT_INDEX_KEY, 0)) || 0;
	const mnemonicSource = await MnemonicAccountSource.createNew({
		password,
		entropyInput: VaultStorage.entropy,
	});
	const mnemonicAccounts = [];
	for (let i = 0; i <= currentMnemonicIndex; i++) {
		const derivationPath = makeDerivationPath(i);
		const keyPair = deriveKeypairFromSeed(VaultStorage.mnemonicSeedHex, derivationPath);
		mnemonicAccounts.push(
			MnemonicAccount.createNew({ keyPair, derivationPath, sourceID: mnemonicSource.id }),
		);
	}
	return { mnemonicSource, mnemonicAccounts };
}

async function makeImportedAccounts(password: string) {
	const importedKeyPairs = VaultStorage.getImportedKeys();
	if (!importedKeyPairs) {
		throw new Error('Failed to load imported accounts, vault is locked');
	}
	return Promise.all(
		importedKeyPairs.map((keyPair) =>
			ImportedAccount.createNew({ password, keyPair: keyPair.export() }),
		),
	);
}

async function makeLedgerAccounts(password: string) {
	const ledgerAccounts = await getSavedLedgerAccounts();
	return Promise.all(
		ledgerAccounts.map(({ address, derivationPath, publicKey }) =>
			LedgerAccount.createNew({ address, derivationPath, password, publicKey }),
		),
	);
}

async function makeQredoAccounts(password: string) {
	const qredoSources = [];
	const qredoAccounts: Omit<QredoSerializedAccount, 'id'>[] = [];
	for (const aQredoConnection of await getAllQredoConnections()) {
		const refreshToken = VaultStorage.getQredoToken(aQredoConnection.id);
		if (!refreshToken) {
			throw new Error(
				`Failed to load qredo account (${aQredoConnection.id}), refresh token not found`,
			);
		}
		const aQredoSource = await QredoAccountSource.createNew({
			password,
			apiUrl: aQredoConnection.apiUrl,
			organization: aQredoConnection.organization,
			origin: aQredoConnection.origin,
			service: aQredoConnection.service,
			refreshToken,
			originFavIcon: aQredoConnection.originFavIcon || '',
		});
		qredoSources.push(aQredoSource);
		for (const aWallet of aQredoConnection.accounts) {
			qredoAccounts.push({
				...aWallet,
				type: 'qredo',
				lastUnlockedOn: null,
				sourceID: aQredoSource.id,
				selected: false,
				nickname: null,
			});
		}
	}
	return { qredoSources, qredoAccounts };
}

function withID<T extends Omit<SerializedAccount, 'id'>>(anAccount: T) {
	return {
		...anAccount,
		id: makeUniqueKey(),
	};
}

export async function doMigration(password: string) {
	await VaultStorage.unlock(password);
	const currentStatus = await getStatus();
	if (currentStatus === 'required') {
		statusCache = 'inProgress';
		try {
			const db = await getDB();
			const currentActiveAccountAddress = await getActiveAccountAddress();
			const { mnemonicAccounts, mnemonicSource } = await makeMnemonicAccounts(password);
			const importedAccounts = await makeImportedAccounts(password);
			const ledgerAccounts = await makeLedgerAccounts(password);
			const { qredoAccounts, qredoSources } = await makeQredoAccounts(password);
			await db.transaction('rw', db.accounts, db.accountSources, async () => {
				await MnemonicAccountSource.save(mnemonicSource, { skipBackup: true, skipEventEmit: true });
				await db.accounts.bulkPut(mnemonicAccounts.map(withID));
				await db.accounts.bulkPut(importedAccounts.map(withID));
				await db.accounts.bulkPut(ledgerAccounts.map(withID));
				for (const aQredoSource of qredoSources) {
					await QredoAccountSource.save(aQredoSource, { skipBackup: true, skipEventEmit: true });
				}
				await db.accounts.bulkPut(qredoAccounts.map(withID));
				if (currentActiveAccountAddress) {
					const accountToSetSelected = await db.accounts.get({
						address: currentActiveAccountAddress,
					});
					if (accountToSetSelected) {
						await db.accounts
							.where('id')
							.notEqual(accountToSetSelected.id)
							.modify({ selected: false });
						await db.accounts.update(accountToSetSelected.id, { selected: true });
					}
				}
				await Dexie.waitFor(setToLocalStorage(migrationDoneStorageKey, true));
			});
			statusCache = 'ready';
			backupDB();
			accountSourcesEvents.emit('accountSourcesChanged');
			accountsEvents.emit('accountsChanged');
		} catch (e) {
			statusCache = 'required';
			throw e;
		}
	}
	await VaultStorage.lock();
}
