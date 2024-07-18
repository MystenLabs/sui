// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Transaction } from '@mysten/sui/transactions';
import { config } from 'dotenv';

import { DeepBookMarketMaker } from './deepbookMarketMaker.js';

// Load private key from .env file
config();

(async () => {
	const privateKey = process.env.PRIVATE_KEY;
	if (!privateKey) {
		throw new Error('Private key not found');
	}

	const mmClient = new DeepBookMarketMaker(privateKey, 'testnet');

	mmClient.dbClient.addBalanceManager(
		'MANAGER_1', // Key of manager
		'0x9f4acee19891c08ec571629df0a81786a8df72f71f4e38d860564c9e54265179', // Address of manager
	);

	const tx = new Transaction();

	// Read only call
	console.log(await mmClient.dbClient.checkManagerBalance('MANAGER_1', 'SUI'));
	console.log(await mmClient.dbClient.getLevel2Range('SUI_DBUSDC', 0.1, 100, true));

	// Balance manager contract call
	mmClient.dbClient.balanceManager.depositIntoManager('MANAGER_1', 1, 'SUI')(tx);

	// Example PTB call
	mmClient.flashLoanExample(tx);

	let res = await mmClient.signAndExecute(tx);

	console.dir(res, { depth: null });
})();
