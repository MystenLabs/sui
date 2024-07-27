// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { Transaction, TransactionResult } from '@mysten/sui/transactions';

import type { Pool } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';

export class FlashLoanContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	borrowAndReturnBaseAsset =
		(
			pool: Pool,
			borrowAmount: number,
			add: <T>(tx: Transaction, flashLoan: TransactionResult[1]) => T,
		) =>
		(tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
			const [baseCoinResult, flashLoan] = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::borrow_flashloan_base`,
				arguments: [tx.object(pool.address), tx.pure.u64(borrowAmount * baseCoin.scalar)],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});

			const result = add(tx, flashLoan);

			// Execute other move calls as necessary

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::return_flashloan_base`,
				arguments: [tx.object(pool.address), baseCoinResult, flashLoan],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});

			return result;
		};

	borrowAndReturnQuoteAsset =
		(
			pool: Pool,
			borrowAmount: number,
			add: <T>(tx: Transaction, flashLoan: TransactionResult[1]) => T,
		) =>
		(tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);

			const [quoteCoinResult, flashLoan] = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::borrow_flashloan_quote`,
				arguments: [tx.object(pool.address), tx.pure.u64(borrowAmount * quoteCoin.scalar)],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});

			const result = add(tx, flashLoan);

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::return_flashloan_quote`,
				arguments: [tx.object(pool.address), quoteCoinResult, flashLoan],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});

			return result;
		};
}
