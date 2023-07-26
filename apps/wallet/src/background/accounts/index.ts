// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/sui.js/utils';
import { isSigningAccount, type SerializedAccount } from './Account';
import { ImportedAccount } from './ImportedAccount';
import { LedgerAccount } from './LedgerAccount';
import { MnemonicAccount } from './MnemonicAccount';
import { QredoAccount } from './QredoAccount';
import { getAccountSourceByID } from '../account-sources';
import { MnemonicAccountSource } from '../account-sources/MnemonicAccountSource';
import { type UiConnection } from '../connections/UiConnection';
import {
	deleteStorageEntity,
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
		case LedgerAccount.isOfType(account):
			return new LedgerAccount({ id: account.id });
		case QredoAccount.isOfType(account):
			return new QredoAccount({ id: account.id });
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

export async function getAccountByAddress(address: string) {
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

async function getQreqoAccountsToDelete<T extends SerializedAccount>(accounts: Omit<T, 'id'>[]) {
	const accountIDsToDelete: string[] = [];
	const allCurrentQredoAccounts = await Promise.all(
		(await getAllAccounts())
			.filter((anAccount): anAccount is QredoAccount => anAccount instanceof QredoAccount)
			.map(async (anAccount) => ({ accountID: anAccount.id, sourceID: await anAccount.sourceID })),
	);
	const newAccountsQredoSourceIDs = new Set();
	for (const aNewAccount of accounts) {
		if (aNewAccount.type === 'qredo' && 'sourceID' in aNewAccount) {
			newAccountsQredoSourceIDs.add(aNewAccount.sourceID);
		}
	}
	newAccountsQredoSourceIDs.forEach((anAccountSourceID) => {
		for (const { accountID, sourceID } of allCurrentQredoAccounts) {
			if (sourceID === anAccountSourceID) {
				accountIDsToDelete.push(accountID);
			}
		}
	});
	return accountIDsToDelete;
}

export async function addNewAccounts<T extends SerializedAccount>(accounts: Omit<T, 'id'>[]) {
	const accountInstances = [];
	const accountsToDelete = await getQreqoAccountsToDelete(accounts);
	for (const anAccountToAdd of accounts) {
		for (const anAccount of await getAllAccounts()) {
			if (
				anAccountToAdd.type === 'qredo' &&
				anAccount instanceof QredoAccount &&
				'sourceID' in anAccountToAdd &&
				anAccountToAdd.sourceID === (await anAccount.sourceID)
			) {
				// The existing account will be deleted after adding the new one
				continue;
			}
			if (
				(await anAccount.address) === anAccountToAdd.address &&
				anAccount.type === anAccountToAdd.type
			) {
				// allow importing accounts that have the same address but are of different type
				// probably it's an edge case and we used to see this problem with importing
				// accounts that were exported from the mnemonic while testing
				throw new Error(`Duplicated account ${anAccountToAdd.address}`);
			}
		}
		const id = makeUniqueKey();
		await setStorageEntity<SerializedAccount>({ ...anAccountToAdd, id });
		const accountInstance = await getAccountByID(id);
		if (!accountInstance) {
			throw new Error(`Something went wrong account with id ${id} not found`);
		}
		accountInstances.push(accountInstance);
	}
	for (const anAccountID of accountsToDelete) {
		await deleteStorageEntity(anAccountID);
	}
	return accountInstances;
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
	// TODO implement
	return false;
}
