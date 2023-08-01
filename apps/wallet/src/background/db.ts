// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Dexie, { type Table } from 'dexie';
import { exportDB } from 'dexie-export-import';
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

export const db = new DB();

db.on('ready', async (vipDB) => {
	const theDB = vipDB as DB;
	const isPopulated = !!(await theDB.settings.get('isPopulated'))?.value;
	if (isPopulated) {
		return;
	}
	try {
		const backup = await getFromLocalStorage<string>(dbLocalStorageBackupKey);
		if (backup) {
			captureException(new Error('IndexedDB is empty, attempting to restore from backup'), {
				extra: { backupSize: backup.length },
			});
			await theDB.import(new Blob([backup], { type: 'application/json' }));
		}
		await theDB.settings.put({ setting: 'isPopulated', value: true });
	} catch (e) {
		captureException(e);
	}
});

export async function backupDB() {
	try {
		const backup = await (await exportDB(db)).text();
		await setToLocalStorage(dbLocalStorageBackupKey, backup);
	} catch (e) {
		captureException(e);
	}
}
