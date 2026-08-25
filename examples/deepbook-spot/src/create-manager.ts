// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#create-manager
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookTestnetClient } from './client.js';

// Create and share a BalanceManager. After execution, read its object ID from
// the transaction effects (the created object whose type ends in
// `BalanceManager`) and pass it to the client as a named manager to trade with.
export function createBalanceManager(client: DeepBookTestnetClient): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.balanceManager.createAndShareBalanceManager());
	return tx;
}
// docs::/#create-manager
