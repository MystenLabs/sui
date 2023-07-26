// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	Account,
	type SerializedAccount,
	type PasswordUnLockableAccount,
	type SerializedUIAccount,
} from './Account';
import { QredoAccountSource } from '../account-sources/QredoAccountSource';
import { type Wallet } from '_src/shared/qredo-api';

export interface QredoSerializedAccount extends SerializedAccount, Wallet {
	type: 'qredo';
	sourceID: string;
	publicKey: string;
}

export interface QredoSerializedUiAccount extends SerializedUIAccount, Wallet {
	type: 'qredo';
	publicKey: string;
	sourceID: string;
}

export function isQredoAccountSerializedUI(
	account: SerializedUIAccount,
): account is QredoSerializedUiAccount {
	return account.type === 'qredo';
}

type EphemeralData = {
	unlocked: true;
};

export class QredoAccount
	extends Account<QredoSerializedAccount, EphemeralData>
	implements PasswordUnLockableAccount
{
	readonly unlockType = 'password' as const;

	static isOfType(serialized: SerializedAccount): serialized is QredoSerializedAccount {
		return serialized.type === 'qredo';
	}

	constructor({ id }: { id: string }) {
		super({ type: 'qredo', id });
	}

	async isLocked(): Promise<boolean> {
		return (await (await this.#getQredoSource()).isLocked()) || !(await this.getEphemeralValue());
	}

	lock(): Promise<void> {
		return this.clearEphemeralValue();
	}

	async passwordUnlock(password: string): Promise<void> {
		await (await this.#getQredoSource()).unlock(password);
		return this.setEphemeralValue({ unlocked: true });
	}

	async toUISerialized(): Promise<QredoSerializedUiAccount> {
		const { id, type, address, publicKey, sourceID, labels, network, walletID } =
			await this.getStoredData();
		return {
			id,
			type,
			address,
			isLocked: await this.isLocked(),
			publicKey,
			sourceID,
			labels,
			network,
			walletID,
		};
	}

	get sourceID() {
		if (!this.cachedData) {
			this.cachedData = this.getStoredData();
		}
		return this.cachedData.then(({ sourceID }) => sourceID);
	}

	get walletID() {
		if (!this.cachedData) {
			this.cachedData = this.getStoredData();
		}
		return this.cachedData.then(({ walletID }) => walletID);
	}

	async #getQredoSource() {
		return new QredoAccountSource((await this.getStoredData()).sourceID);
	}
}
