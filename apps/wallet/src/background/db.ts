// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Dexie, { type Table } from 'dexie';
import { exportDB, importDB } from 'dexie-export-import';
import { type AccountSourceSerialized } from './account-sources/AccountSource';
import { type SerializedAccount } from './accounts/Account';
import { captureException } from './sentry';
import { getFromLocalStorage, setToLocalStorage } from './storage-utils';

const dbName = 'SuiWallet DB';
const dbLocalStorageBackupKey = 'indexed-db-backup';

class DB extends Dexie {
	accountSources!: Table<AccountSourceSerialized, string>;
	accounts!: Table<SerializedAccount, string>;
	settings!: Table<{ value: boolean; setting: string }, string>;

	constructor() {
		super(dbName);
		this.version(1).stores({
			accountSources: 'id, type',
			accounts: 'id, type, address, sourceID',
			settings: 'setting',
		});
	}
}

async function init() {
	const db = new DB();
	const isPopulated = !!(await db.settings.get('isPopulated'))?.value;
	if (!isPopulated) {
		try {
			const backup = await getFromLocalStorage<string>(dbLocalStorageBackupKey);
			if (backup) {
				captureException(new Error('IndexedDB is empty, attempting to restore from backup'), {
					extra: { backupSize: backup.length },
				});
				await db.delete();
				(await importDB(new Blob([backup], { type: 'application/json' }))).close();
				await db.open();
			}
			await db.settings.put({ setting: 'isPopulated', value: true });
		} catch (e) {
			captureException(e);
		}
	}
	if (!db.isOpen()) {
		await db.open();
	}
	return db;
}
let initPromise: ReturnType<typeof init> | null = null;
export const getDB = () => {
	if (!initPromise) {
		initPromise = init();
	}
	return initPromise;
};

export async function backupDB() {
	try {
		const backup = await (await exportDB(await getDB())).text();
		await setToLocalStorage(dbLocalStorageBackupKey, backup);
	} catch (e) {
		captureException(e);
	}
}
