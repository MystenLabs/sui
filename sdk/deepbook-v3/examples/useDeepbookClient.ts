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

	// Initialize with balance managers if created
	const balanceManagers = {
		MANAGER_1: {
			address: '',
			tradeCap: '',
		},
	};
	const mmClient = new DeepBookMarketMaker(privateKey, 'testnet', balanceManagers);

	const tx = new Transaction();

	// Read only call
	console.log(await mmClient.checkManagerBalance('MANAGER_1', 'SUI'));
	console.log(await mmClient.getLevel2Range('SUI_DBUSDC', 0.1, 100, true));

	// Balance manager contract call
	mmClient.balanceManager.depositIntoManager('MANAGER_1', 1, 'SUI')(tx);

	// Example PTB call
	mmClient.placeLimitOrderExample(tx);
	mmClient.flashLoanExample(tx);

	let res = await mmClient.signAndExecute(tx);

	console.dir(res, { depth: null });
})();
