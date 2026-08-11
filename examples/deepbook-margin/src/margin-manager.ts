// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookMarginClient } from './client.js';

// docs::#find-manager
// Reuse an existing MarginManager instead of minting a new one on every run.
// `getMarginManagerIdsForOwner` returns the managers the registry tracks for an
// owner across every pool. A MarginManager is bound to one DeepBook pool, so
// return only the one for `poolKey`: reading a manager's state with `poolKey`
// succeeds when the manager belongs to that pool and aborts otherwise, so a
// manager for a different pool is skipped. Returning the first manager of any
// pool would wire the rest of the flow to the wrong pool. Treat the lookup as
// best-effort and also persist the ID you create, because a new manager each run
// leaves orphaned shared objects that fragment your collateral.
export async function findMarginManagerId(
	client: DeepBookMarginClient,
	owner: string,
	poolKey: string,
): Promise<string | undefined> {
	const ids = await client.deepbook.getMarginManagerIdsForOwner(owner);
	for (const id of ids) {
		try {
			const states = await client.deepbook.getMarginManagerStates({ [id]: poolKey });
			if (Object.keys(states).length > 0) return id;
		} catch {
			// Reading with `poolKey` aborts for a manager bound to another pool.
		}
	}
	return undefined;
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
