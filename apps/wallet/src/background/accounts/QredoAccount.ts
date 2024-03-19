// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Wallet } from '_src/shared/qredo-api';

import { QredoAccountSource } from '../account-sources/QredoAccountSource';
import {
	Account,
	type PasswordUnlockableAccount,
	type SerializedAccount,
	type SerializedUIAccount,
} from './Account';

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
	implements PasswordUnlockableAccount
{
	readonly unlockType = 'password' as const;

	static isOfType(serialized: SerializedAccount): serialized is QredoSerializedAccount {
		return serialized.type === 'qredo';
	}

	constructor({ id, cachedData }: { id: string; cachedData?: QredoSerializedAccount }) {
		super({ type: 'qredo', id, cachedData });
	}

	async isLocked(): Promise<boolean> {
		return (await (await this.#getQredoSource()).isLocked()) || !(await this.getEphemeralValue());
	}

	async lock(allowRead = false): Promise<void> {
		await this.clearEphemeralValue();
		await this.onLocked(allowRead);
	}

	async passwordUnlock(password?: string): Promise<void> {
		if (!password) {
			throw new Error('Missing password to unlock the account');
		}
		await (await this.#getQredoSource()).unlock(password);
		await this.setEphemeralValue({ unlocked: true });
		await this.onUnlocked();
	}

	async verifyPassword(password: string): Promise<void> {
		const qredoSource = await this.#getQredoSource();
		await qredoSource.verifyPassword(password);
	}

	async toUISerialized(): Promise<QredoSerializedUiAccount> {
		const {
			id,
			type,
			address,
			publicKey,
			sourceID,
			labels,
			network,
			walletID,
			selected,
			nickname,
		} = await this.getStoredData();
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
			lastUnlockedOn: await this.lastUnlockedOn,
			selected,
			nickname,
			isPasswordUnlockable: true,
			isKeyPairExportable: false,
		};
	}

	get sourceID() {
		return this.getCachedData().then(({ sourceID }) => sourceID);
	}

	get walletID() {
		return this.getCachedData().then(({ walletID }) => walletID);
	}

	async #getQredoSource() {
		return new QredoAccountSource((await this.getStoredData()).sourceID);
	}
}
