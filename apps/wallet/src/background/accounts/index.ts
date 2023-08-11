// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/sui.js/utils';
import Dexie from 'dexie';
import { isPasswordUnLockable, isSigningAccount, type SerializedAccount } from './Account';
import { ImportedAccount } from './ImportedAccount';
import { LedgerAccount } from './LedgerAccount';
import { MnemonicAccount } from './MnemonicAccount';
import { QredoAccount } from './QredoAccount';
import { accountsEvents } from './events';
import { ZkAccount } from './zk/ZkAccount';
import { getAccountSourceByID } from '../account-sources';
import { MnemonicAccountSource } from '../account-sources/MnemonicAccountSource';
import { type UiConnection } from '../connections/UiConnection';
import { backupDB, getDB } from '../db';
import { getFromLocalStorage, makeUniqueKey } from '../storage-utils';
import { createMessage, type Message } from '_src/shared/messaging/messages';
import {
	type MethodPayload,
	isMethodPayload,
} from '_src/shared/messaging/messages/payloads/MethodPayload';

function toAccount(account: SerializedAccount) {
	if (MnemonicAccount.isOfType(account)) {
		return new MnemonicAccount({ id: account.id, cachedData: account });
	}
	if (ImportedAccount.isOfType(account)) {
		return new ImportedAccount({ id: account.id, cachedData: account });
	}
	if (LedgerAccount.isOfType(account)) {
		return new LedgerAccount({ id: account.id, cachedData: account });
	}
	if (QredoAccount.isOfType(account)) {
		return new QredoAccount({ id: account.id, cachedData: account });
	}
	if (ZkAccount.isOfType(account)) {
		return new ZkAccount({ id: account.id, cachedData: account });
	}
	throw new Error(`Unknown account of type ${account.type}`);
}

export async function getAllAccounts(filter?: { sourceID: string }) {
	const db = await getDB();
	let accounts;
	if (filter?.sourceID) {
		accounts = await db.accounts.where('sourceID').equals(filter.sourceID);
	} else {
		accounts = db.accounts;
	}
	return (await accounts.toArray()).map(toAccount);
}

export async function getAccountByID(id: string) {
	const serializedAccount = await (await getDB()).accounts.get(id);
	if (!serializedAccount) {
		return null;
	}
	return toAccount(serializedAccount);
}

export async function getAccountsByAddress(address: string) {
	return (await (await getDB()).accounts.where('address').equals(address).toArray()).map(toAccount);
}

export async function getAllSerializedUIAccounts() {
	return Promise.all((await getAllAccounts()).map((anAccount) => anAccount.toUISerialized()));
}

export async function isAccountsInitialized() {
	return (await (await getDB()).accounts.count()) > 0;
}

export async function getActiveAccount() {
	const accountID = await getFromLocalStorage<string>('active-account-id-key');
	if (!accountID) {
		return null;
	}
	return getAccountByID(accountID);
}

async function deleteQredoAccounts<T extends SerializedAccount>(accounts: Omit<T, 'id'>[]) {
	const newAccountsQredoSourceIDs = new Set<string>();
	const walletIDsSet = new Set<string>();
	for (const aNewAccount of accounts) {
		if (
			aNewAccount.type === 'qredo' &&
			'sourceID' in aNewAccount &&
			typeof aNewAccount.sourceID === 'string' &&
			'walletID' in aNewAccount &&
			typeof aNewAccount.walletID === 'string'
		) {
			newAccountsQredoSourceIDs.add(aNewAccount.sourceID);
			walletIDsSet.add(aNewAccount.walletID);
		}
	}
	if (!newAccountsQredoSourceIDs.size) {
		return 0;
	}
	return (await Dexie.waitFor(getDB())).accounts
		.where('sourceID')
		.anyOf(Array.from(newAccountsQredoSourceIDs.values()))
		.filter(
			(anExistingAccount) =>
				anExistingAccount.type === 'qredo' &&
				'walletID' in anExistingAccount &&
				typeof anExistingAccount.walletID === 'string' &&
				!walletIDsSet.has(anExistingAccount.walletID),
		)
		.delete();
}

export async function addNewAccounts<T extends SerializedAccount>(accounts: Omit<T, 'id'>[]) {
	const db = await getDB();
	const accountsCreated = await db.transaction('rw', db.accounts, async () => {
		// delete all existing qredo accounts that have the same sourceID (come from the same connection)
		// and not in the new accounts list
		await deleteQredoAccounts(accounts);
		const accountInstances = [];
		for (const anAccountToAdd of accounts) {
			let id = '';
			const existingSameAddressAccounts = await getAccountsByAddress(anAccountToAdd.address);
			for (const anExistingAccount of existingSameAddressAccounts) {
				if (
					anAccountToAdd.type === 'qredo' &&
					anExistingAccount instanceof QredoAccount &&
					'sourceID' in anAccountToAdd &&
					anAccountToAdd.sourceID === (await Dexie.waitFor(anExistingAccount.sourceID))
				) {
					id = anExistingAccount.id;
					continue;
				}
				if (
					(await Dexie.waitFor(anExistingAccount.address)) === anAccountToAdd.address &&
					anExistingAccount.type === anAccountToAdd.type
				) {
					// allow importing accounts that have the same address but are of different type
					// probably it's an edge case and we used to see this problem with importing
					// accounts that were exported from the mnemonic while testing
					throw new Error(`Duplicated account ${anAccountToAdd.address}`);
				}
			}
			id = id || makeUniqueKey();
			await db.accounts.put({ ...anAccountToAdd, id });
			const accountInstance = await Dexie.waitFor(getAccountByID(id));
			if (!accountInstance) {
				throw new Error(`Something went wrong account with id ${id} not found`);
			}
			accountInstances.push(accountInstance);
		}
		return accountInstances;
	});
	await backupDB();
	accountsEvents.emit('accountsChanged');
	return accountsCreated;
}

export async function accountsHandleUIMessage(msg: Message, uiConnection: UiConnection) {
	const { payload } = msg;
	if (isMethodPayload(payload, 'lockAccountSourceOrAccount')) {
		const account = await getAccountByID(payload.args.id);
		if (account) {
			await account.lock();
			await uiConnection.send(createMessage({ type: 'done' }, msg.id));
			return true;
		}
	}
	if (isMethodPayload(payload, 'unlockAccountSourceOrAccount')) {
		const { id, password } = payload.args;
		const account = await getAccountByID(id);
		if (account) {
			if (isPasswordUnLockable(account)) {
				if (!password) {
					throw new Error('Missing password to unlock the account');
				}
				await account.passwordUnlock(password);
			} else {
				await account.unlock();
			}
			await uiConnection.send(createMessage({ type: 'done' }, msg.id));
			return true;
		}
	}
	if (isMethodPayload(payload, 'signData')) {
		const { id, data } = payload.args;
		const account = await getAccountByID(id);
		if (!account) {
			throw new Error(`Account with address ${id} not found`);
		}
		if (!isSigningAccount(account)) {
			throw new Error(`Account with address ${id} is not a signing account`);
		}
		await uiConnection.send(
			createMessage<MethodPayload<'signDataResponse'>>(
				{
					type: 'method-payload',
					method: 'signDataResponse',
					args: { signature: await account.signData(fromB64(data)) },
				},
				msg.id,
			),
		);
		return true;
	}
	if (isMethodPayload(payload, 'createAccounts')) {
		let newSerializedAccounts: Omit<SerializedAccount, 'id'>[] = [];
		const { type } = payload.args;
		if (type === 'mnemonic-derived') {
			const { sourceID } = payload.args;
			const accountSource = await getAccountSourceByID(payload.args.sourceID);
			if (!accountSource) {
				throw new Error(`Account source ${sourceID} not found`);
			}
			if (!(accountSource instanceof MnemonicAccountSource)) {
				throw new Error(`Invalid account source type`);
			}
			newSerializedAccounts.push(await accountSource.deriveAccount());
		} else if (type === 'imported') {
			newSerializedAccounts.push(await ImportedAccount.createNew(payload.args));
		} else if (type === 'ledger') {
			const { password, accounts } = payload.args;
			for (const aLedgerAccount of accounts) {
				newSerializedAccounts.push(await LedgerAccount.createNew({ ...aLedgerAccount, password }));
			}
		} else if (type === 'zk') {
			newSerializedAccounts.push(await ZkAccount.createNew(payload.args));
		} else {
			throw new Error(`Unknown accounts type to create ${type}`);
		}
		const newAccounts = await addNewAccounts(newSerializedAccounts);
		await uiConnection.send(
			createMessage<MethodPayload<'accountsCreatedResponse'>>(
				{
					method: 'accountsCreatedResponse',
					type: 'method-payload',
					args: {
						accounts: await Promise.all(
							newAccounts.map(async (aNewAccount) => await aNewAccount.toUISerialized()),
						),
					},
				},
				msg.id,
			),
		);
		return true;
	}
	return false;
}
