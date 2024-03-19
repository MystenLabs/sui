// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createMessage, type Message } from '_src/shared/messaging/messages';
import {
	isMethodPayload,
	type MethodPayload,
} from '_src/shared/messaging/messages/payloads/MethodPayload';
import { toEntropy } from '_src/shared/utils/bip39';

import { type UiConnection } from '../connections/UiConnection';
import { getDB } from '../db';
import { type QredoConnectIdentity } from '../qredo/types';
import { isSameQredoConnection } from '../qredo/utils';
import {
	type AccountSource,
	type AccountSourceSerialized,
	type AccountSourceType,
} from './AccountSource';
import { MnemonicAccountSource } from './MnemonicAccountSource';
import { QredoAccountSource } from './QredoAccountSource';

function toAccountSource(accountSource: AccountSourceSerialized) {
	if (MnemonicAccountSource.isOfType(accountSource)) {
		return new MnemonicAccountSource(accountSource.id);
	}
	if (QredoAccountSource.isOfType(accountSource)) {
		return new QredoAccountSource(accountSource.id);
	}
	throw new Error(`Unknown account source of type ${accountSource.type}`);
}

export async function getAccountSources(filter?: { type: AccountSourceType }) {
	const db = await getDB();
	return (
		await (filter?.type
			? await db.accountSources.where('type').equals(filter.type).sortBy('createdAt')
			: await db.accountSources.toCollection().sortBy('createdAt'))
	).map(toAccountSource);
}

export async function getAccountSourceByID(id: string) {
	const serializedAccountSource = await (await getDB()).accountSources.get(id);
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
				await MnemonicAccountSource.save(
					await MnemonicAccountSource.createNew({
						password,
						entropyInput: entropy ? toEntropy(entropy) : undefined,
					}),
				)
			).toUISerialized();
		default: {
			throw new Error(`Unknown Account source type ${type}`);
		}
	}
}

export async function getQredoAccountSource(filter: string | QredoConnectIdentity) {
	let accountSource: AccountSource | null = null;
	if (typeof filter === 'string') {
		accountSource = await getAccountSourceByID(filter);
	} else {
		const accountSourceSerialized = (
			await (await getDB()).accountSources.where('type').equals('qredo').toArray()
		)
			.filter(QredoAccountSource.isOfType)
			.find((anAccountSource) => isSameQredoConnection(filter, anAccountSource));
		accountSource = accountSourceSerialized
			? new QredoAccountSource(accountSourceSerialized.id)
			: null;
	}
	if (!accountSource || !(accountSource instanceof QredoAccountSource)) {
		return null;
	}
	return accountSource;
}

export async function lockAllAccountSources() {
	const allAccountSources = await getAccountSources();
	for (const anAccountSource of allAccountSources) {
		await anAccountSource.lock();
	}
}

export async function accountSourcesHandleUIMessage(msg: Message, uiConnection: UiConnection) {
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

	if (isMethodPayload(payload, 'unlockAccountSourceOrAccount')) {
		const { id, password } = payload.args;
		const accountSource = await getAccountSourceByID(id);
		if (accountSource) {
			if (!password) {
				throw new Error('Missing password');
			}
			await accountSource.unlock(password);
			await uiConnection.send(createMessage({ type: 'done' }, msg.id));
			return true;
		}
	}
	if (isMethodPayload(payload, 'lockAccountSourceOrAccount')) {
		const accountSource = await getAccountSourceByID(payload.args.id);
		if (accountSource) {
			await accountSource.lock();
			await uiConnection.send(createMessage({ type: 'done' }, msg.id));
			return true;
		}
	}
	if (isMethodPayload(payload, 'getAccountSourceEntropy')) {
		const accountSource = await getAccountSourceByID(payload.args.accountSourceID);
		if (!accountSource) {
			throw new Error('Account source not found');
		}
		if (!(accountSource instanceof MnemonicAccountSource)) {
			throw new Error('Invalid account source type');
		}
		await uiConnection.send(
			createMessage<MethodPayload<'getAccountSourceEntropyResponse'>>(
				{
					type: 'method-payload',
					method: 'getAccountSourceEntropyResponse',
					args: { entropy: await accountSource.getEntropy(payload.args.password) },
				},
				msg.id,
			),
		);
		return true;
	}
	if (isMethodPayload(payload, 'verifyPasswordRecoveryData')) {
		const { accountSourceID, entropy } = payload.args.data;
		const accountSource = await getAccountSourceByID(accountSourceID);
		if (!accountSource) {
			throw new Error('Account source not found');
		}
		if (!(accountSource instanceof MnemonicAccountSource)) {
			throw new Error('Invalid account source type');
		}
		await accountSource.verifyRecoveryData(entropy);
		uiConnection.send(createMessage({ type: 'done' }, msg.id));
		return true;
	}
	return false;
}
