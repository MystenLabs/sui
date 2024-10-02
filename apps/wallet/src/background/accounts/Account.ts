// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Serializable } from '_src/shared/cryptography/keystore';
import { toSerializedSignature, type Keypair } from '@mysten/sui/cryptography';
import { blake2b } from '@noble/hashes/blake2b';

import { setupAutoLockAlarm } from '../auto-lock-accounts';
import { getDB } from '../db';
import {
	clearEphemeralValue,
	getEphemeralValue,
	setEphemeralValue,
} from '../session-ephemeral-values';
import { accountsEvents } from './events';

export type AccountType = 'mnemonic-derived' | 'imported' | 'ledger' | 'qredo' | 'zkLogin';

export abstract class Account<
	T extends SerializedAccount = SerializedAccount,
	V extends Serializable | null = Serializable,
> {
	readonly id: string;
	readonly type: AccountType;
	// optimization to avoid accessing storage for properties that don't change
	private cachedData: Promise<T> | null = null;

	constructor({ id, type, cachedData }: { id: string; type: AccountType; cachedData?: T }) {
		this.id = id;
		this.type = type;
		if (cachedData) {
			this.cachedData = Promise.resolve(cachedData);
		}
	}

	abstract lock(allowRead: boolean): Promise<void>;
	/**
	 * Indicates if the account is unlocked and allows write actions (eg. signing)
	 */
	abstract isLocked(): Promise<boolean>;
	abstract toUISerialized(): Promise<SerializedUIAccount>;

	get address() {
		return this.getCachedData().then(({ address }) => address);
	}

	get lastUnlockedOn() {
		return this.getCachedData().then(({ lastUnlockedOn }) => lastUnlockedOn);
	}

	get publicKey() {
		return this.getCachedData().then(({ publicKey }) => publicKey);
	}

	protected getCachedData() {
		if (!this.cachedData) {
			this.cachedData = this.getStoredData();
		}
		return this.cachedData;
	}

	protected async getStoredData() {
		const data = await (await getDB()).accounts.get(this.id);
		if (!data) {
			throw new Error(`Account data not found. (id: ${this.id})`);
		}
		return data as T;
	}

	protected async generateSignature(data: Uint8Array, keyPair: Keypair) {
		const digest = blake2b(data, { dkLen: 32 });
		const pubkey = keyPair.getPublicKey();
		const signature = await keyPair.sign(digest);
		const signatureScheme = keyPair.getKeyScheme();
		return toSerializedSignature({
			signature,
			signatureScheme,
			publicKey: pubkey,
		});
	}

	protected getEphemeralValue(): Promise<V | null> {
		return getEphemeralValue<NonNullable<V>>(this.id);
	}

	protected setEphemeralValue(value: V) {
		if (!value) {
			return;
		}
		return setEphemeralValue(this.id, value);
	}

	protected clearEphemeralValue() {
		return clearEphemeralValue(this.id);
	}

	protected async onUnlocked() {
		await setupAutoLockAlarm();
		await (await getDB()).accounts.update(this.id, { lastUnlockedOn: Date.now() });
		accountsEvents.emit('accountStatusChanged', { accountID: this.id });
	}

	protected async onLocked(allowRead: boolean) {
		// skip clearing last unlocked value to allow read access
		// when possible (last unlocked withing time limits)
		if (allowRead) {
			return;
		}
		await (await getDB()).accounts.update(this.id, { lastUnlockedOn: null });
		accountsEvents.emit('accountStatusChanged', { accountID: this.id });
	}

	public async setNickname(nickname: string | null) {
		await (await getDB()).accounts.update(this.id, { nickname });
		accountsEvents.emit('accountStatusChanged', { accountID: this.id });
	}
}

export interface SerializedAccount {
	readonly id: string;
	readonly type: AccountType;
	readonly address: string;
	readonly publicKey: string | null;
	readonly lastUnlockedOn: number | null;
	/**
	 * indicates if it's the selected account in the UI (active account)
	 */
	readonly selected: boolean;
	readonly nickname: string | null;
	readonly createdAt: number;
}

export interface SerializedUIAccount {
	readonly id: string;
	readonly type: AccountType;
	readonly address: string;
	/**
	 * This means the account is not able to sign when isLocked is true (write locked)
	 */
	readonly isLocked: boolean;
	readonly publicKey: string | null;
	/**
	 * Timestamp of the last time the account was unlocked. It is cleared when the account is locked
	 * because of a user action (manual lock) or lock timer.
	 * This is used to determine if the account is locked for read or not. (eg. lastUnlockedOn more than 4 hours ago -> read locked)
	 */
	readonly lastUnlockedOn: number | null;
	/**
	 * indicates if it's the selected account in the UI (active account)
	 */
	readonly selected: boolean;
	readonly nickname: string | null;
	readonly isPasswordUnlockable: boolean;
	readonly isKeyPairExportable: boolean;
}

export interface PasswordUnlockableAccount {
	readonly unlockType: 'password';
	passwordUnlock(password?: string): Promise<void>;
	verifyPassword(password: string): Promise<void>;
}

export function isPasswordUnLockable(account: unknown): account is PasswordUnlockableAccount {
	return !!(
		account &&
		typeof account === 'object' &&
		'passwordUnlock' in account &&
		'unlockType' in account &&
		account.unlockType === 'password'
	);
}

export interface SigningAccount {
	readonly canSign: true;
	signData(data: Uint8Array): Promise<string>;
}

export function isSigningAccount(account: any): account is SigningAccount {
	return 'signData' in account && 'canSign' in account && account.canSign === true;
}

export interface KeyPairExportableAccount {
	readonly exportableKeyPair: true;
	exportKeyPair(password: string): Promise<string>;
}

export function isKeyPairExportableAccount(account: any): account is KeyPairExportableAccount {
	return (
		'exportKeyPair' in account &&
		'exportableKeyPair' in account &&
		account.exportableKeyPair === true
	);
}
