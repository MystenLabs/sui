// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';

import { DeepBookClient } from '../src/index.js'; // Adjust import source accordingly

(async () => {
	const env = 'mainnet';

	// Initialize with balance managers if needed
	const balanceManagers = {
		MANAGER_1: {
			address: '0x344c2734b1d211bd15212bfb7847c66a3b18803f3f5ab00f5ff6f87b6fe6d27d',
			tradeCap: '',
		},
	};

	const dbClient = new DeepBookClient({
		address: '0x0',
		env: env,
		client: new SuiClient({
			url: getFullnodeUrl(env),
		}),
		balanceManagers: balanceManagers,
	});

	console.log(await dbClient.checkManagerBalance('MANAGER_1', 'SUI'));
	console.log(await dbClient.checkManagerBalance('MANAGER_1', 'USDC'));
	console.log(await dbClient.checkManagerBalance('MANAGER_1', 'WUSDT'));
	console.log(await dbClient.checkManagerBalance('MANAGER_1', 'WUSDC'));
	console.log(await dbClient.checkManagerBalance('MANAGER_1', 'BETH'));
	console.log(await dbClient.checkManagerBalance('MANAGER_1', 'DEEP'));
})();
