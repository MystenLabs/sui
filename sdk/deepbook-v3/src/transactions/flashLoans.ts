// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { Transaction } from '@mysten/sui/transactions';

import type { Pool } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';

export class FlashLoanContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	borrowBaseAsset = (pool: Pool, borrowAmount: number) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const [baseCoinResult, flashLoan] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::borrow_flashloan_base`,
			arguments: [tx.object(pool.address), tx.pure.u64(borrowAmount * baseCoin.scalar)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		return [baseCoinResult, flashLoan] as const;
	};

	returnBaseAsset = (pool: Pool, baseCoinInput: any, flashLoan: any) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::return_flashloan_base`,
			arguments: [tx.object(pool.address), baseCoinInput, flashLoan],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	borrowQuoteAsset = (pool: Pool, borrowAmount: number) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const [quoteCoinResult, flashLoan] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::borrow_flashloan_quote`,
			arguments: [tx.object(pool.address), tx.pure.u64(borrowAmount * quoteCoin.scalar)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		return [quoteCoinResult, flashLoan] as const;
	};

	returnQuoteAsset = (pool: Pool, quoteCoinInput: any, flashLoan: any) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::return_flashloan_quote`,
			arguments: [tx.object(pool.address), quoteCoinInput, flashLoan],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};
}
