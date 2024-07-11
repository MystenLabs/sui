// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Transaction } from '@mysten/sui/transactions';

import type { Pool } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';

export class FlashLoanContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	borrowBaseAsset = (pool: Pool, borrowAmount: number, tx: Transaction = new Transaction()) => {
		const baseScalar = pool.baseCoin.scalar;
		const [baseCoinResult, flashLoan] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::borrow_flashloan_base`,
			arguments: [tx.object(pool.address), tx.pure.u64(borrowAmount * baseScalar)],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});
		return [baseCoinResult, flashLoan];
	};

	returnBaseAsset = (
		pool: Pool,
		baseCoin: any,
		flashLoan: any,
		tx: Transaction = new Transaction(),
	) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::return_flashloan_base`,
			arguments: [tx.object(pool.address), baseCoin, flashLoan],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});
	};

	borrowQuoteAsset = (pool: Pool, borrowAmount: number, tx: Transaction = new Transaction()) => {
		const quoteScalar = pool.quoteCoin.scalar;
		const [quoteCoinResult, flashLoan] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::borrow_flashloan_quote`,
			arguments: [tx.object(pool.address), tx.pure.u64(borrowAmount * quoteScalar)],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});
		return [quoteCoinResult, flashLoan];
	};

	returnQuoteAsset = (
		pool: Pool,
		quoteCoin: any,
		flashLoan: any,
		tx: Transaction = new Transaction(),
	) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::return_flashloan_quote`,
			arguments: [tx.object(pool.address), quoteCoin, flashLoan],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});
	};
}
