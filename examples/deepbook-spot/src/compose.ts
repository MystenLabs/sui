// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { OrderType } from '@mysten/deepbook-v3';
import type { DeepBookTestnetClient } from './client.js';

// docs::#fund-order
// Deposit into the BalanceManager and place an order in the SAME transaction, so
// both settle atomically: if the order placement aborts, the deposit rolls back
// with it, and you never end up funded but orderless or vice versa. Each
// `tx.add` appends a command to the one programmable transaction block.
export function fundAndOrder(
	client: DeepBookTestnetClient,
	managerKey: string,
	depositCoinKey: string,
	depositAmount: number,
	poolKey: string,
	clientOrderId: string, // numeric string, encoded as u64
	price: number,
	quantity: number,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.balanceManager.depositIntoManager(managerKey, depositCoinKey, depositAmount));
	tx.add(
		client.deepbook.deepBook.placeLimitOrder({
			poolKey,
			balanceManagerKey: managerKey,
			clientOrderId,
			price,
			quantity,
			isBid: false,
			orderType: OrderType.POST_ONLY,
		}),
	);
	return tx;
}
// docs::/#fund-order

// docs::#flash-loan
// Borrow, use, and repay a flash loan in one transaction. `borrowBaseAsset`
// returns the borrowed coin and a `FlashLoan` hot potato: a struct with no
// abilities that cannot be stored, dropped, or transferred, so the only way to
// finish the transaction is to pass it back to `returnBaseAsset`. Forget to
// return it and the whole block aborts. The borrowed coin flows through your
// logic (the "use" step) and the repaid coin covers the borrow; here the use
// step is a no-op so the example is self-contained, but this is exactly where a
// call into another protocol would go, under the same all-or-nothing rule.
export function flashLoanRoundTrip(
	client: DeepBookTestnetClient,
	poolKey: string,
	borrowAmount: number,
	recipient: string,
): Transaction {
	const tx = new Transaction();
	const [borrowed, flashLoan] = tx.add(
		client.deepbook.flashLoans.borrowBaseAsset(poolKey, borrowAmount),
	);
	// ... use `borrowed` here: swap it, arbitrage, or call another protocol ...
	const remainder = tx.add(
		client.deepbook.flashLoans.returnBaseAsset(poolKey, borrowAmount, borrowed, flashLoan),
	);
	tx.transferObjects([remainder], recipient);
	return tx;
}
// docs::/#flash-loan
