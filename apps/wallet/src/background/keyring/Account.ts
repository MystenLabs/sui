// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type DerivedAccount, type SerializedDerivedAccount } from './DerivedAccount';
import { type ImportedAccount, type SerializedImportedAccount } from './ImportedAccount';
import { type LedgerAccount, type SerializedLedgerAccount } from './LedgerAccount';
import { type QredoAccount, type SerializedQredoAccount } from './QredoAccount';

export enum AccountType {
	IMPORTED = 'IMPORTED',
	DERIVED = 'DERIVED',
	LEDGER = 'LEDGER',
	QREDO = 'QREDO',
}

export type SerializedAccount =
	| SerializedImportedAccount
	| SerializedDerivedAccount
	| SerializedLedgerAccount
	| SerializedQredoAccount;

export interface Account {
	readonly type: AccountType;
	readonly address: string;
	toJSON(): SerializedAccount;
	getPublicKey(): string | null;
}

export function isImportedOrDerivedAccount(
	account: Account,
): account is ImportedAccount | DerivedAccount {
	return isImportedAccount(account) || isDerivedAccount(account);
}

export function isImportedAccount(account: Account): account is ImportedAccount {
	return account.type === AccountType.IMPORTED;
}

export function isDerivedAccount(account: Account): account is DerivedAccount {
	return account.type === AccountType.DERIVED;
}

export function isLedgerAccount(account: Account): account is LedgerAccount {
	return account.type === AccountType.LEDGER;
}
export function isQredoAccount(account: Account): account is QredoAccount {
	return account.type === AccountType.QREDO;
}
