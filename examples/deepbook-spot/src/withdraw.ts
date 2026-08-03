// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#withdraw-spot
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookTestnetClient } from './client.js';

// Withdraw a coin's full settled balance from the manager back to `recipient`.
// After a fill, settled balances hold your proceeds until you withdraw them.
export function withdrawAll(
	client: DeepBookTestnetClient,
	managerKey: string,
	coinKey: string,
	recipient: string,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.balanceManager.withdrawAllFromManager(managerKey, coinKey, recipient));
	return tx;
}
// docs::/#withdraw-spot
