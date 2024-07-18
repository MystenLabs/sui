// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Transaction } from '@mysten/sui/transactions';

import { DeepBookMarketMaker } from './deepbookMarketMaker.js';

(async () => {
	const dbMM = new DeepBookMarketMaker(
		'',
		'testnet',
	);

	dbMM.dbClient.addBalanceManager(
		'MANAGER_1',
		'0x9f4acee19891c08ec571629df0a81786a8df72f71f4e38d860564c9e54265179',
	);

	const tx = new Transaction();

	let res = await dbMM.signAndExecuteWithClientAndSigner(tx);
	// remove from name

	console.dir(res, { depth: null });
})();
