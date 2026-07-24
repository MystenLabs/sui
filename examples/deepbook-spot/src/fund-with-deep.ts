// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#fund-deep
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookTestnetClient } from './client.js';

// Deposit DEEP into a BalanceManager. DEEP held in the manager pays trading
// fees, and staking it in a pool unlocks the taker-fee discount. `deepAmount` is
// in whole DEEP; the SDK scales it to the coin's 6 decimals.
export function fundManagerWithDeep(
	client: DeepBookTestnetClient,
	managerKey: string,
	deepAmount: number,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.balanceManager.depositIntoManager(managerKey, 'DEEP', deepAmount));
	return tx;
}
// docs::/#fund-deep
