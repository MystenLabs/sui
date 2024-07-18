// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Transaction } from '@mysten/sui/transactions';

import { Utils } from '../test/helper/utils.js'; // Correct path
import { DeepBookMarketMaker } from './deepbookMarketMaker.js';

(async () => {
	const keypair = Utils.getSignerFromPK(
		'',
	);

	const mm = new DeepBookMarketMaker(keypair, 'testnet');

	mm.dbClient.addBalanceManager(
		'MANAGER_1',
		'0x9f4acee19891c08ec571629df0a81786a8df72f71f4e38d860564c9e54265179',
	);

	const tx = new Transaction();

	mm.dbClient.addBalanceManager(
		'MANAGER_1',
		'0x9f4acee19891c08ec571629df0a81786a8df72f71f4e38d860564c9e54265179',
	);

	let res = Utils.signAndExecuteWithClientAndSigner(tx, mm.suiClient, keypair);

	console.dir(res, { depth: null });
})();
