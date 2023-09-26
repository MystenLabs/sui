// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type DerivedAccount, type SerializedDerivedAccount } from './DerivedAccount';
import { type ImportedAccount, type SerializedImportedAccount } from './ImportedAccount';
import { type LedgerAccount, type SerializedLedgerAccount } from './LedgerAccount';
import { type QredoAccount, type SerializedQredoAccount } from './QredoAccount';

/**
 * @deprecated
 */
export enum AccountType {
	IMPORTED = 'IMPORTED',
	DERIVED = 'DERIVED',
	LEDGER = 'LEDGER',
	QREDO = 'QREDO',
}

/**
 * @deprecated
 */
export type SerializedAccount =
	| SerializedImportedAccount
	| SerializedDerivedAccount
	| SerializedLedgerAccount
	| SerializedQredoAccount;

/**
 * @deprecated
 */
export interface Account {
	readonly type: AccountType;
	readonly address: string;
	toJSON(): SerializedAccount;
	getPublicKey(): string | null;
}

/**
 * @deprecated
 */
export function isImportedOrDerivedAccount(
	account: Account,
): account is ImportedAccount | DerivedAccount {
	return isImportedAccount(account) || isDerivedAccount(account);
}

/**
 * @deprecated
 */
export function isImportedAccount(account: Account): account is ImportedAccount {
	return account.type === AccountType.IMPORTED;
}

/**
 * @deprecated
 */
export function isDerivedAccount(account: Account): account is DerivedAccount {
	return account.type === AccountType.DERIVED;
}

/**
 * @deprecated
 */
export function isLedgerAccount(account: Account): account is LedgerAccount {
	return account.type === AccountType.LEDGER;
}
/**
 * @deprecated
 */
export function isQredoAccount(account: Account): account is QredoAccount {
	return account.type === AccountType.QREDO;
}
/**
 * @deprecated
 */
export function isSerializedQredoAccount(
	account: SerializedAccount,
): account is SerializedQredoAccount {
	return account.type === AccountType.QREDO;
}
