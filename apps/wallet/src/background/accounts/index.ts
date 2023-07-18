// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedAccount } from './Account';
import { MnemonicAccount } from './MnemonicAccount';
import {
	getAllStoredEntities,
	getStorageEntity,
	setStorageEntity,
} from '../storage-entities-utils';
import { getFromLocalStorage, makeUniqueKey } from '../storage-utils';
import { type Message } from '_src/shared/messaging/messages';

function toAccount(account: SerializedAccount) {
	switch (true) {
		case MnemonicAccount.isOfType(account):
			return new MnemonicAccount({ id: account.id });
		default:
			throw new Error(`Unknown account of type ${account.type}`);
	}
}

export async function getAllAccounts() {
	return (await getAllStoredEntities<SerializedAccount>('account-entity')).map(toAccount);
}

export async function getAccountByID(id: string) {
	const serializedAccount = await getStorageEntity<SerializedAccount>(id);
	if (!serializedAccount) {
		return null;
	}
	return toAccount(serializedAccount);
}

export async function getAllSerializedUIAccounts() {
	return Promise.all((await getAllAccounts()).map((anAccount) => anAccount.toUISerialized()));
}

export async function isAccountsInitialized() {
	return (await getAllAccounts()).length > 0;
}

export async function getActiveAccount() {
	const accountID = await getFromLocalStorage<string>('active-account-id-key');
	if (!accountID) {
		return null;
	}
	return getAccountByID(accountID);
}

export async function addNewAccount<T extends SerializedAccount>(account: Omit<T, 'id'>) {
	for (const anAccount of await getAllAccounts()) {
		if ((await anAccount.address) === account.address) {
			throw new Error(`Duplicated account ${account.address}`);
			// TODO: handle imported keys accounts duplication or maybe any other case?
		}
	}
	const id = makeUniqueKey();
	await setStorageEntity<SerializedAccount>({ ...account, id });
	const accountInstance = await getAccountByID(id);
	if (!accountInstance) {
		throw new Error(`Something went wrong account with id ${id} not found`);
	}
	return accountInstance;
	// TODO: emit event
}

export async function handleUIMessage(msg: Message) {
	// TODO implement
	return false;
}
