// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, type SuiAddress } from '@mysten/sui.js';
import { isSigningAccount, type SerializedAccount } from './Account';
import { ImportedAccount } from './ImportedAccount';
import { MnemonicAccount } from './MnemonicAccount';
import { getAccountSourceByID } from '../account-sources';
import { MnemonicAccountSource } from '../account-sources/MnemonicAccountSource';
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
		case ImportedAccount.isOfType(account):
			return new ImportedAccount({ id: account.id });
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
		// allow importing accounts that have the same address but are of different type
		// probably it's an edge case and we used to see this problem with importing
		// accounts that were exported from the mnemonic while testing
		if ((await anAccount.address) === account.address && anAccount.type === account.type) {
			throw new Error(`Duplicated account ${account.address}`);
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
	if (isMethodPayload(payload, 'createAccount')) {
		let newSerializedAccount: Omit<SerializedAccount, 'id'>;
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
			newSerializedAccount = await accountSource.deriveAccount();
		} else if (type === 'imported') {
			newSerializedAccount = await ImportedAccount.createNew(payload.args);
		} else {
			throw new Error(`Unknown account type to create ${type}`);
		}
		const newAccount = await addNewAccount(newSerializedAccount);
		await uiConnection.send(
			createMessage<MethodPayload<'accountCreatedResponse'>>(
				{
					method: 'accountCreatedResponse',
					type: 'method-payload',
					args: { account: await newAccount.toUISerialized() },
				},
				msg.id,
			),
		);
		return true;
	}
	// TODO implement
	return false;
}
