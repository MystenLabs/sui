// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type AccountSourceSerialized, type AccountSourceType } from './AccountSource';
import { MnemonicAccountSource } from './MnemonicAccountSource';
import { type UiConnection } from '../connections/UiConnection';
import { getAllStoredEntities, getStorageEntity } from '../storage-entities-utils';
import { type Message, createMessage } from '_src/shared/messaging/messages';
import {
	type MethodPayload,
	isMethodPayload,
} from '_src/shared/messaging/messages/payloads/MethodPayload';
import { toEntropy } from '_src/shared/utils/bip39';

function toAccountSource(accountSource: AccountSourceSerialized) {
	switch (true) {
		case MnemonicAccountSource.isOfType(accountSource):
			return new MnemonicAccountSource(accountSource.id);
		default:
			throw new Error(`Unknown account source of type ${accountSource.type}`);
	}
}

export async function getAccountSources(filter?: { type: AccountSourceType }) {
	const all = (await getAllStoredEntities<AccountSourceSerialized>('account-source-entity')).map(
		toAccountSource,
	);
	if (!filter?.type) {
		return all;
	}
	return all.filter((anAccountSource) => anAccountSource.type === filter.type);
}

export async function getAccountSourceByID(id: string) {
	const serializedAccountSource = await getStorageEntity<AccountSourceSerialized>(id);
	if (!serializedAccountSource) {
		return null;
	}
	return toAccountSource(serializedAccountSource);
}

export async function getAllSerializedUIAccountSources() {
	return Promise.all(
		(await getAccountSources()).map((anAccountSource) => anAccountSource.toUISerialized()),
	);
}

async function createAccountSource({
	type,
	params: { password, entropy },
}: MethodPayload<'createAccountSource'>['args']) {
	switch (type) {
		case 'mnemonic':
			return (
				await MnemonicAccountSource.createNew({
					password,
					entropyInput: entropy ? toEntropy(entropy) : undefined,
				})
			).toUISerialized();
		default: {
			throw new Error(`Unknown Account source type ${type}`);
		}
	}
}

export async function handleUIMessage(msg: Message, uiConnection: UiConnection) {
	const { payload } = msg;
	if (isMethodPayload(payload, 'createAccountSource')) {
		await uiConnection.send(
			createMessage<MethodPayload<'accountSourceCreationResponse'>>(
				{
					method: 'accountSourceCreationResponse',
					type: 'method-payload',
					args: { accountSource: await createAccountSource(payload.args) },
				},
				msg.id,
			),
		);
		return true;
	}
	if (isMethodPayload(payload, 'unlockAccountSource')) {
		const { id, type, password } = payload.args;
		const accountSource = await getAccountSourceByID(id);
		if (!accountSource) {
			throw new Error(`Account source not found, ${id} - ${type}`);
		}
		await accountSource.unlock(password);
		await uiConnection.send(createMessage({ type: 'done' }, msg.id));
		// TODO: emit event to notify UI?
		return true;
	}
	if (isMethodPayload(payload, 'deriveMnemonicAccount')) {
		const { sourceID } = payload.args;
		const accountSource = await getAccountSourceByID(sourceID);
		if (!accountSource) {
			throw new Error(`Account source ${sourceID} not found`);
		}
		if (!(accountSource instanceof MnemonicAccountSource)) {
			throw new Error(`Invalid account source type`);
		}
		const newAccount = await accountSource.deriveAccount();
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
	return false;
}
