// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	type Keypair,
	toSerializedSignature,
	type SerializedSignature,
	type SuiAddress,
} from '@mysten/sui.js';
import { blake2b } from '@noble/hashes/blake2b';
import {
	clearEphemeralValue,
	getEphemeralValue,
	setEphemeralValue,
} from '../session-ephemeral-values';
import {
	type StorageEntity,
	getStorageEntity,
	updateStorageEntity,
} from '../storage-entities-utils';
import { type Serializable } from '_src/shared/cryptography/keystore';

export type AccountType = 'mnemonic-derived' | 'imported' | 'ledger' | 'qredo';

export abstract class Account<
	T extends SerializedAccount = SerializedAccount,
	V extends Serializable | null = Serializable,
> {
	readonly id: string;
	readonly type: AccountType;
	// optimization to avoid accessing storage for properties that don't change
	protected cachedData: Promise<T> | null = null;

	constructor({ id, type }: { id: string; type: AccountType }) {
		this.id = id;
		this.type = type;
	}

	abstract lock(): Promise<void>;
	abstract isLocked(): Promise<boolean>;
	abstract toUISerialized(): Promise<SerializedUIAccount>;

	protected async getStoredData() {
		const data = await getStorageEntity<T>(this.id, 'account-entity');
		if (!data) {
			throw new Error(`Account data not found. (id: ${this.id})`);
		}
		return data;
	}

	protected generateSignature(data: Uint8Array, keyPair: Keypair) {
		const digest = blake2b(data, { dkLen: 32 });
		const pubkey = keyPair.getPublicKey();
		const signature = keyPair.signData(digest);
		const signatureScheme = keyPair.getKeyScheme();
		return toSerializedSignature({
			signature,
			signatureScheme,
			pubKey: pubkey,
		});
	}

	updateStoredData(update: Parameters<typeof updateStorageEntity<T>>['1']) {
		return updateStorageEntity<T>(this.id, 'account-entity', update);
	}

	getEphemeralValue(): Promise<V | null> {
		return getEphemeralValue<NonNullable<V>>(this.id);
	}

	setEphemeralValue(value: V) {
		if (!value) {
			return;
		}
		return setEphemeralValue(this.id, value);
	}

	clearEphemeralValue() {
		return clearEphemeralValue(this.id);
	}

	get address() {
		if (!this.cachedData) {
			this.cachedData = this.getStoredData();
		}
		return this.cachedData.then(({ address }) => address);
	}
}

export interface SerializedAccount extends StorageEntity {
	readonly storageEntityType: 'account-entity';
	readonly type: AccountType;
	readonly address: SuiAddress;
	readonly publicKey: string | null;
}

export interface SerializedUIAccount {
	readonly id: string;
	readonly type: AccountType;
	readonly address: SuiAddress;
	readonly isLocked: boolean;
	readonly publicKey: string | null;
}

export interface PasswordUnLockableAccount {
	readonly unlockType: 'password';
	passwordUnlock(password: string): Promise<void>;
}

export function isPasswordUnLockable(account: any): account is PasswordUnLockableAccount {
	return (
		'passwordUnlock' in account && 'unlockType' in account && account.unlockType === 'password'
	);
}

export interface SigningAccount {
	readonly canSign: true;
	signData(data: Uint8Array): Promise<SerializedSignature>;
}

export function isSigningAccount(account: any): account is SigningAccount {
	return 'signData' in account && 'canSign' in account && account.canSign === true;
}
