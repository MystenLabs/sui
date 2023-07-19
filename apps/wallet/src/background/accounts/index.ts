// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, type SuiAddress } from '@mysten/sui.js';
import { isSigningAccount, type SerializedAccount } from './Account';
import { MnemonicAccount } from './MnemonicAccount';
import { type UiConnection } from '../connections/UiConnection';
import {
	getAllStoredEntities,
	getStorageEntity,
	setStorageEntity,
} from '../storage-entities-utils';
import { getFromLocalStorage, makeUniqueKey } from '../storage-utils';
import { createMessage, type Message } from '_src/shared/messaging/messages';
import {
	type MethodPayload,
	isMethodPayload,
} from '_src/shared/messaging/messages/payloads/MethodPayload';

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
	const serializedAccount = await getStorageEntity<SerializedAccount>(id, 'account-entity');
	if (!serializedAccount) {
		return null;
	}
	return toAccount(serializedAccount);
}

export async function getAccountByAddress(address: SuiAddress) {
	const allAccounts = await getAllAccounts();
	for (const anAccount of allAccounts) {
		if ((await anAccount.address) === address) {
			return anAccount;
		}
	}
	return null;
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
			await account.passwordUnlock(password);
			// TODO: Auto lock timer
			await uiConnection.send(createMessage({ type: 'done' }, msg.id));
			// TODO: emit event to notify UI?
			return true;
		}
	}
	if (isMethodPayload(payload, 'signData')) {
		const { address, data } = payload.args;
		const account = await getAccountByAddress(address);
		if (!account) {
			throw new Error(`Account with address ${address} not found`);
		}
		if (!isSigningAccount(account)) {
			throw new Error(`Account with address ${address} is not a signing account`);
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
	// TODO implement
	return false;
}
