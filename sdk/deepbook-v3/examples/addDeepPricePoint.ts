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
			address: '0x6149bfe6808f0d6a9db1c766552b7ae1df477f5885493436214ed4228e842393',
			tradeCap: '',
		},
	};

	const mmClient = new DeepBookMarketMaker(
		privateKey,
		'testnet',
		balanceManagers,
		process.env.ADMIN_CAP,
	);

	const tx = new Transaction();

	// Balance manager contract call
	mmClient.deepBook.addDeepPricePoint('SUI_DBUSDC', 'DEEP_DBUSDC')(tx);

	const res = await mmClient.signAndExecute(tx);

	console.dir(res, { depth: null });
})();
