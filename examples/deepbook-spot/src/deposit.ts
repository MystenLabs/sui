// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#deposit
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookTestnetClient } from './client.js';

// Deposit any accepted coin into the BalanceManager, addressed by SDK coin key
// (for example 'DEEP' or 'SUI'). The amount is in whole coins; the SDK scales it
// to the coin's decimals.
export function depositAsset(
	client: DeepBookTestnetClient,
	managerKey: string,
	coinKey: string,
	amount: number,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.balanceManager.depositIntoManager(managerKey, coinKey, amount));
	return tx;
}
// docs::/#deposit

// docs::#sized-deposit
// Never deposit more than the wallet holds. `depositIntoManager` sources coins
// with coinWithBalance, which throws at build time otherwise:
//   Insufficient balance of <coin> for owner <address>. Required: X, Available: Y
// Size the deposit to your actual balance, and leave a reserve when the coin is
// SUI so gas still has funds.
export async function safeDepositAmount(
	client: DeepBookTestnetClient,
	owner: string,
	coinType: string,
	decimals: number,
	want: number,
	reserve = 0,
): Promise<number> {
	const b = await client.core.getBalance({ owner, coinType });
	const have = Number(b?.balance?.balance ?? '0') / 10 ** decimals;
	return Math.max(0, Math.min(want, have - reserve));
}
// docs::/#sized-deposit
