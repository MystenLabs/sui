// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	clearEphemeralValue,
	getEphemeralValue,
	setEphemeralValue,
} from '../session-ephemeral-values';
import {
	updateStorageEntity,
	type StorageEntity,
	getStorageEntity,
} from '../storage-entities-utils';
import { type Serializable } from '_src/shared/cryptography/keystore';

export type AccountSourceType = 'mnemonic' | 'qredo';

export abstract class AccountSource<T extends AccountSourceSerialized, V extends Serializable> {
	readonly id: string;
	readonly type: AccountSourceType;

	constructor({ id, type }: { type: AccountSourceType; id: string }) {
		this.id = id;
		this.type = type;
	}

	abstract toUISerialized(): Promise<AccountSourceSerializedUI>;
	abstract isLocked(): Promise<boolean>;
	abstract lock(): Promise<void>;

	async getStoredData() {
		const data = await getStorageEntity<T>(this.id, 'account-source-entity');
		if (!data) {
			throw new Error(`Account data not found. (id: ${this.id})`);
		}
		return data;
	}

	updateStoredData(update: Parameters<typeof updateStorageEntity<T>>['2']) {
		return updateStorageEntity<T>(this.id, 'account-source-entity', update);
	}

	getEphemeralValue(): Promise<V | null> {
		return getEphemeralValue<V>(this.id);
	}

	setEphemeralValue(value: V) {
		return setEphemeralValue(this.id, value);
	}

	clearEphemeralValue() {
		return clearEphemeralValue(this.id);
	}
}

export interface AccountSourceSerialized extends StorageEntity {
	readonly storageEntityType: 'account-source-entity';
	readonly id: string;
	readonly type: AccountSourceType;
}

export type AccountSourceSerializedUI = {
	readonly id: string;
	readonly type: AccountSourceType;
	readonly isLocked: boolean;
};
