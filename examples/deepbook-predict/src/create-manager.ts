// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#create-manager
import type {
	CreateManagerReceipt,
	DecodableTransactionResult,
} from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';

// Each trader holds one canonical account: a shared `AccountWrapper` holding an
// `Account`. The builder keeps the legacy DeepBook balance-manager name, but the
// object it creates is the account wrapper.
export function createAccount(): Transaction {
	return client.predict.tx.createManager();
}

// The wrapper ID is derived from the owner address, so you can compute it before
// the transaction lands. No chain read.
export function accountWrapperId(owner: string): string {
	return client.predict.wrapperIdFor(owner);
}

// First-time setup in a single PTB: create the wrapper, deposit, and share it.
// `owner` must be the address that signs, because the wrapper is derived from
// the transaction sender. This aborts if the account already exists, so use it
// only on the create path.
export function createAndFund(owner: string, amountUsdc: number): Transaction {
	return client.predict.tx.deposit(owner, amountUsdc, { create: true });
}

// Fund an account that already exists. The DUSDC is sourced from the owner's
// coin objects and address balance together.
export function deposit(owner: string, amountUsdc: number): Transaction {
	return client.predict.tx.deposit(owner, amountUsdc);
}

// Take DUSDC back out of the account. It lands in the owner's address balance;
// pass `{ toCoinObject: true }` when you need a discrete coin object instead.
export function withdrawToWallet(owner: string, amountUsdc: number): Transaction {
	return client.predict.tx.withdraw(owner, amountUsdc);
}

// The decoders are pure and touch no network. Execute the create transaction
// with events included, then read the IDs off the receipt.
export function decodeCreated(result: DecodableTransactionResult): CreateManagerReceipt {
	return client.predict.decode.createManager(result);
}

// The account's internal custody balance in DUSDC, as a decimal number. This is
// the balance a mint is debited from, not the owner's wallet balance.
export async function accountBalance(owner: string): Promise<number> {
	return client.predict.read.balance(owner);
}
// docs::/#create-manager
