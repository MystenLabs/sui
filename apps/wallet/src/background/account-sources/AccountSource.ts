// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Serializable } from '_src/shared/cryptography/keystore';

import { getDB } from '../db';
import {
	clearEphemeralValue,
	getEphemeralValue,
	setEphemeralValue,
} from '../session-ephemeral-values';

export type AccountSourceType = 'mnemonic' | 'qredo';

export abstract class AccountSource<
	T extends AccountSourceSerialized = AccountSourceSerialized,
	V extends Serializable = Serializable,
> {
	readonly id: string;
	readonly type: AccountSourceType;

	constructor({ id, type }: { type: AccountSourceType; id: string }) {
		this.id = id;
		this.type = type;
	}

	abstract toUISerialized(): Promise<AccountSourceSerializedUI>;
	abstract isLocked(): Promise<boolean>;
	abstract lock(): Promise<void>;

	protected async getStoredData() {
		const data = await (await getDB()).accountSources.get(this.id);
		if (!data) {
			throw new Error(`Account data not found. (id: ${this.id})`);
		}
		return data as T;
	}

	protected getEphemeralValue(): Promise<V | null> {
		return getEphemeralValue<V>(this.id);
	}

	protected setEphemeralValue(value: V) {
		return setEphemeralValue(this.id, value);
	}

	protected clearEphemeralValue() {
		return clearEphemeralValue(this.id);
	}
}

export interface AccountSourceSerialized {
	readonly id: string;
	readonly type: AccountSourceType;
	readonly createdAt: number;
}

export type AccountSourceSerializedUI = {
	readonly id: string;
	readonly type: AccountSourceType;
	readonly isLocked: boolean;
};
