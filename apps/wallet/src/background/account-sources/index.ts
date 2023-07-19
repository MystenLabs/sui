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
	const serializedAccountSource = await getStorageEntity<AccountSourceSerialized>(
		id,
		'account-source-entity',
	);
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
			await accountSource.unlock(password);
			// TODO: Auto lock timer
			await uiConnection.send(createMessage({ type: 'done' }, msg.id));
			// TODO: emit event to notify UI?
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
	return false;
}
