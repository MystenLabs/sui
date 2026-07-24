// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookMarginClient } from './client.js';

// docs::#find-manager
// Reuse an existing MarginManager instead of minting a new one on every run.
// `getMarginManagerIdsForOwner` returns the managers the registry tracks for an
// owner. A MarginManager is tied to one DeepBook pool, so filter by the pool you
// intend to trade. Treat the lookup as best-effort and also persist the ID you
// create: a new manager each run leaves orphaned shared objects that fragment
// your collateral.
export async function findMarginManagerId(
	client: DeepBookMarginClient,
	owner: string,
): Promise<string | undefined> {
	const ids = await client.deepbook.getMarginManagerIdsForOwner(owner);
	return ids[0];
}
// docs::/#find-manager

// docs::#create-manager
// Create a MarginManager for a pool. A single `margin_manager::new` call creates
// the manager, shares it, and registers it in the MarginRegistry, so there is no
// separate registration step for the normal path. After execution, read the
// created shared object whose type contains `MarginManager` from the effects,
// persist its ID, and pass it to the client under `marginManagers` so later
// steps address it by key.
export function createMarginManager(
	client: DeepBookMarginClient,
	poolKey: string,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.marginManager.newMarginManager(poolKey));
	return tx;
}
// docs::/#create-manager
