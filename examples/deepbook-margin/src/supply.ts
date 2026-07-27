// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookMarginClient } from './client.js';

// docs::#create-cap
// A SupplierCap authorizes supplying to (and withdrawing from) margin pools and
// tracks your supply position. It is a reusable owned object: mint it once, read
// its ID from the created objects in the effects, persist it, and reuse it for
// every later supply. Minting a fresh cap each time fragments your supply
// position across caps, so gate creation on an existing persisted ID.
export function createSupplierCap(client: DeepBookMarginClient, owner: string): Transaction {
	const tx = new Transaction();
	const cap = tx.add(client.deepbook.marginPool.mintSupplierCap());
	tx.transferObjects([cap], owner);
	return tx;
}
// docs::/#create-cap

// docs::#supply
// Supply an asset to its margin pool. The SDK sources the coin from your wallet,
// so keep a gas reserve when supplying SUI. Supplying needs no borrow liquidity
// and is capped only by the pool's supply cap. `supplierCapId` is the reusable
// cap you persisted; `referralId` is optional.
export function supplyToPool(
	client: DeepBookMarginClient,
	coinKey: string,
	supplierCapId: string,
	amount: number,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.marginPool.supplyToMarginPool(coinKey, tx.object(supplierCapId), amount));
	return tx;
}
// docs::/#supply
