// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Dexie from 'dexie';
import {
	MnemonicAccountSource,
	deriveKeypairFromSeed,
	makeDerivationPath,
} from './account-sources/MnemonicAccountSource';
import { QredoAccountSource } from './account-sources/QredoAccountSource';
import { addNewAccounts } from './accounts';
import { ImportedAccount } from './accounts/ImportedAccount';
import { LedgerAccount } from './accounts/LedgerAccount';
import { MnemonicAccount } from './accounts/MnemonicAccount';
import { type QredoSerializedAccount } from './accounts/QredoAccount';
import { backupDB, getDB } from './db';
import { STORAGE_LAST_ACCOUNT_INDEX_KEY, getSavedLedgerAccounts } from './keyring';
import { VaultStorage } from './keyring/VaultStorage';
import { getAllQredoConnections } from './qredo/storage';
import { getFromLocalStorage, setToLocalStorage } from './storage-utils';
import { NEW_ACCOUNTS_ENABLED } from '_src/shared/constants';

export type Status = 'required' | 'inProgress' | 'ready';

const migrationDoneStorageKey = 'storage-migration-done';

let statusCache: Status | null = null;

export async function getStatus() {
	if (statusCache) {
		return statusCache;
	}
	if (!NEW_ACCOUNTS_ENABLED) {
		return (statusCache = 'ready');
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
			organization: aQredoConnection.origin,
			origin: aQredoConnection.origin,
			service: aQredoConnection.service,
			refreshToken,
		});
		qredoSources.push(aQredoSource);
		for (const aWallet of aQredoConnection.accounts) {
			qredoAccounts.push({
				...aWallet,
				type: 'qredo',
				lastUnlockedOn: null,
				sourceID: aQredoSource.id,
			});
		}
	}
	return { qredoSources, qredoAccounts };
}

export async function doMigration(password: string) {
	await VaultStorage.unlock(password);
	const currentStatus = await getStatus();
	if (currentStatus === 'required') {
		statusCache = 'inProgress';
		try {
			const db = await getDB();
			const { mnemonicAccounts, mnemonicSource } = await makeMnemonicAccounts(password);
			const importedAccounts = await makeImportedAccounts(password);
			const ledgerAccounts = await makeLedgerAccounts(password);
			const { qredoAccounts, qredoSources } = await makeQredoAccounts(password);
			await db.transaction('rw', db.accounts, db.accountSources, async () => {
				await MnemonicAccountSource.save(mnemonicSource, { skipBackup: true });
				await addNewAccounts(mnemonicAccounts, { skipBackup: true });
				await addNewAccounts(importedAccounts, { skipBackup: true });
				await addNewAccounts(ledgerAccounts, { skipBackup: true });
				for (const aQredoSource of qredoSources) {
					await QredoAccountSource.save(aQredoSource, { skipBackup: true });
				}
				await addNewAccounts(qredoAccounts, { skipBackup: true });
				await Dexie.waitFor(setToLocalStorage(migrationDoneStorageKey, true));
			});
			statusCache = 'ready';
			backupDB();
		} catch (e) {
			statusCache = 'required';
			throw e;
		}
	}
	await VaultStorage.lock();
}
