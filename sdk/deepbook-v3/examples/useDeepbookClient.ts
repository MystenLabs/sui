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
			address: '0xc873b1639903ab279b3d20bdb4497c67739e1cb9c4acd6df9be4e6330c9ca8a6',
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

	// Upgrade
	// mmClient.deepBookAdmin.enableVersion(3)(tx);

	// Update to new package

	// Read only call
	// console.log(await mmClient.checkManagerBalance('MANAGER_1', 'SUI'));
	// console.log(await mmClient.getLevel2Range('DEEP_SUI', 0.1, 100, false));
	// console.log(await mmClient.getLevel2TicksFromMid('DEEP_SUI', 5));
	// console.log(await mmClient.accountOpenOrders('SUI_DBUSDC', 'MANAGER_1'));
	// console.log(await mmClient.getOrder('SUI_DBUSDC', '46116878631017952749551614'));

	// Balance manager contract call
	// mmClient.balanceManager.withdrawAllFromManager(
	// 	'MANAGER_1',
	// 	'SUI',
	// 	mmClient.getActiveAddress(),
	// )(tx);
	// mmClient.balanceManager.withdrawAllFromManager(
	// 	'MANAGER_1',
	// 	'DEEP',
	// 	mmClient.getActiveAddress(),
	// )(tx);
	// mmClient.balanceManager.withdrawAllFromManager(
	// 	'MANAGER_1',
	// 	'DBUSDC',
	// 	mmClient.getActiveAddress(),
	// )(tx);
	// mmClient.balanceManager.withdrawAllFromManager(
	// 	'MANAGER_1',
	// 	'DBUSDT',
	// 	mmClient.getActiveAddress(),
	// )(tx);

	// Do the folliwng for upgrades
	// mmClient.deepBookAdmin.enableVersion(5)(tx);
	// mmClient.deepBookAdmin.updateAllowedVersions('DEEP_SUI')(tx);
	// mmClient.deepBookAdmin.updateAllowedVersions('SUI_DBUSDC')(tx);
	// mmClient.deepBookAdmin.updateAllowedVersions('DEEP_DBUSDC')(tx);
	// mmClient.deepBookAdmin.updateAllowedVersions('DBUSDT_DBUSDC')(tx);

	// Switch to new package
	// mmClient.balanceManager.createAndShareBalanceManager()(tx);
	// mmClient.deepBookAdmin.disableVersion(4)(tx);
	// mmClient.balanceManager.depositIntoManager('MANAGER_1', 'SUI', 50)(tx);
	// mmClient.balanceManager.depositIntoManager('MANAGER_1', 'DEEP', 1000)(tx);
	// mmClient.balanceManager.depositIntoManager('MANAGER_1', 'DBUSDC', 1000)(tx);
	// mmClient.balanceManager.depositIntoManager('MANAGER_1', 'DBUSDT', 1000)(tx);

	// Reset Pools
	// mmClient.deepBookAdmin.unregisterPoolAdmin('DEEP_SUI')(tx);
	// mmClient.deepBookAdmin.unregisterPoolAdmin('SUI_DBUSDC')(tx);
	// mmClient.deepBookAdmin.unregisterPoolAdmin('DEEP_DBUSDC')(tx);
	// mmClient.deepBookAdmin.unregisterPoolAdmin('DBUSDT_DBUSDC')(tx);

	// mmClient.deepBookAdmin.createPoolAdmin({
	// 	baseCoinKey: 'DEEP',
	// 	quoteCoinKey: 'SUI',
	// 	tickSize: 0.001,
	// 	lotSize: 0.1,
	// 	minSize: 1,
	// 	whitelisted: true,
	// 	stablePool: false,
	// })(tx);

	// mmClient.deepBookAdmin.createPoolAdmin({
	// 	baseCoinKey: 'SUI',
	// 	quoteCoinKey: 'DBUSDC',
	// 	tickSize: 0.001,
	// 	lotSize: 0.01,
	// 	minSize: 0.1,
	// 	whitelisted: false,
	// 	stablePool: false,
	// })(tx);

	// mmClient.deepBookAdmin.createPoolAdmin({
	// 	baseCoinKey: 'DEEP',
	// 	quoteCoinKey: 'DBUSDC',
	// 	tickSize: 0.001,
	// 	lotSize: 0.1,
	// 	minSize: 1,
	// 	whitelisted: true,
	// 	stablePool: false,
	// })(tx);

	// mmClient.deepBookAdmin.createPoolAdmin({
	// 	baseCoinKey: 'DBUSDT',
	// 	quoteCoinKey: 'DBUSDC',
	// 	tickSize: 0.001,
	// 	lotSize: 0.01,
	// 	minSize: 0.1,
	// 	whitelisted: false,
	// 	stablePool: true,
	// })(tx);

	// mmClient.placeLimitOrderExample(tx);
	// mmClient.deepBook.addDeepPricePoint('SUI_DBUSDC', 'DEEP_DBUSDC')(tx);
	// mmClient.deepBook.addDeepPricePoint('DBUSDT_DBUSDC', 'DEEP_DBUSDC')(tx);
	// mmClient.placeLimitOrderExample2(tx);

	// Test
	// const [base, quote, deep] = mmClient.deepBook.swapExactBaseForQuote({
	// 	poolKey: 'SUI_DBUSDC',
	// 	amount: 0.1,
	// 	deepAmount: 1,
	// 	minOut: 0,
	// })(tx);
	// tx.transferObjects([base, quote, deep], mmClient.getActiveAddress());
	// const [base2, quote2, deep2] = mmClient.deepBook.swapExactBaseForQuote({
	// 	poolKey: 'DBUSDT_DBUSDC',
	// 	amount: 0.1,
	// 	deepAmount: 1,
	// 	minOut: 0,
	// })(tx);
	// tx.transferObjects([base2, quote2, deep2], mmClient.getActiveAddress());

	let res = await mmClient.signAndExecute(tx);

	console.dir(res, { depth: null });
})();
