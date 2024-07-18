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

	const dbMM = new DeepBookMarketMaker(privateKey, 'testnet');

	dbMM.dbClient.addBalanceManager(
		'MANAGER_1', // Key of manager
		'0x9f4acee19891c08ec571629df0a81786a8df72f71f4e38d860564c9e54265179', // Address of manager
	);

	const tx = new Transaction();

	// tx.add(
	// 	await dbMM.dbClient.deepBook.placeLimitOrder({
	// 		poolKey: 'SUI_DBUSDC',
	// 		balanceManager: dbMM.dbClient.getBalanceManager('MANAGER_1'),
	// 		clientOrderId: 888,
	// 		price: 1,
	// 		quantity: 10,
	// 		isBid: false,
	// 	}),
	// );

	const borrowAmount = 1;
	const [deepCoin, flashLoan] = await tx.add(
		dbMM.dbClient.flashLoans.borrowBaseAsset('DEEP_SUI', borrowAmount),
	);

	// Execute trade using borrowed DEEP
	const [baseOut, quoteOut, deepOut] = await tx.add(
		dbMM.dbClient.deepBook.swapExactQuoteForBase({
			poolKey: 'SUI_DBUSDC',
			amount: 0.5,
			deepAmount: 1,
			minOut: 0,
			deepCoin: deepCoin,
		}),
	);

	tx.transferObjects([baseOut, quoteOut, deepOut], dbMM.getActiveAddress());

	// Execute second trade to get back DEEP for repayment
	const [baseOut2, quoteOut2, deepOut2] = await tx.add(
		dbMM.dbClient.deepBook.swapExactQuoteForBase({
			poolKey: 'DEEP_SUI',
			amount: 10,
			deepAmount: 0,
			minOut: 0,
		}),
	);

	tx.transferObjects([quoteOut2, deepOut2], dbMM.getActiveAddress());

	const loanRemain = await tx.add(
		dbMM.dbClient.flashLoans.returnBaseAsset('DEEP_SUI', borrowAmount, baseOut2, flashLoan),
	);
	tx.transferObjects([loanRemain], dbMM.getActiveAddress());

	let res = await dbMM.signAndExecute(tx);

	console.dir(res, { depth: null });
})();
