// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#find-manager
import type { DeepBookTestnetClient } from './client.js';

// Reuse an existing BalanceManager instead of minting a new one on every run.
// `getBalanceManagerIds` returns the managers the DeepBook indexer knows for an
// owner. It can return an empty list for a manager that has never traded or
// before the indexer catches up, so treat it as best-effort: also persist the ID
// you create (in config, a file, or an env var) and reuse that. Creating a new
// manager each run leaves orphaned shared objects and confuses any step that
// lists your managers.
export async function findManagerId(
	client: DeepBookTestnetClient,
	owner: string,
): Promise<string | undefined> {
	const ids = await client.deepbook.getBalanceManagerIds(owner);
	return ids[0];
}
// docs::/#find-manager

// docs::#report-managers
import { deepbookClient } from './client.js';

// Report where a key's funds live across its BalanceManagers. Managers are
// shared objects, so anything you deposit stays in them until you withdraw.
// When a deposit cannot be afforded, the funds are usually stranded in another
// manager, not lost: pass the manager IDs you have created (persist each one)
// and check their balances before erroring.
export async function reportManagers(
	owner: string,
	managerIds: string[],
	coinKeys: string[],
): Promise<void> {
	for (const id of managerIds) {
		const c = deepbookClient(owner, { M: { address: id } });
		const balances = await Promise.all(
			coinKeys.map((k) => c.deepbook.checkManagerBalance('M', k).then((b) => `${k}=${b.balance}`)),
		);
		console.log(`${id}: ${balances.join(' ')}`);
	}
}
// docs::/#report-managers
