// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Transaction } from '@mysten/sui/transactions';

import type { Coin, Pool } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { FLOAT_SCALAR, POOL_CREATION_FEE } from '../utils/config.js';

export class DeepBookAdminContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	#adminCap() {
		const adminCap = this.#config.adminCap;
		if (!adminCap) {
			throw new Error('ADMIN_CAP environment variable not set');
		}
		return adminCap;
	}

	createPoolAdmin = (
		baseCoin: Coin,
		quoteCoin: Coin,
		deepCoinId: string,
		tickSize: number,
		lotSize: number,
		minSize: number,
		whitelisted: boolean,
		stablePool: boolean,
		tx: Transaction = new Transaction(),
	) => {
		const [creationFee] = tx.splitCoins(tx.object(deepCoinId), [tx.pure.u64(POOL_CREATION_FEE)]);

		const baseScalar = baseCoin.scalar;
		const quoteScalar = quoteCoin.scalar;

		const adjustedTickSize = (tickSize * FLOAT_SCALAR * quoteScalar) / baseScalar;
		const adjustedLotSize = lotSize * baseScalar;
		const adjustedMinSize = minSize * baseScalar;

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::create_pool_admin`,
			arguments: [
				tx.object(this.#config.REGISTRY_ID), // registry_id
				tx.pure.u64(adjustedTickSize), // adjusted tick_size
				tx.pure.u64(adjustedLotSize), // adjusted lot_size
				tx.pure.u64(adjustedMinSize), // adjusted min_size
				creationFee, // 0x2::balance::Balance<0x2::sui::SUI>
				tx.pure.bool(whitelisted),
				tx.pure.bool(stablePool),
				tx.object(this.#adminCap()),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return tx;
	};

	unregisterPoolAdmin = (pool: Pool, tx: Transaction = new Transaction()) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::unregister_pool_admin`,
			arguments: [tx.object(this.#config.REGISTRY_ID), tx.object(this.#adminCap())],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});

		return tx;
	};

	// TODO: Needs to be revised after move code is updated
	updateDisabledVersions = (pool: Pool, tx: Transaction = new Transaction()) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::update_disabled_versions`,
			arguments: [
				tx.object(pool.address),
				tx.object(this.#config.REGISTRY_ID),
				tx.object(this.#adminCap()),
			],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});

		return tx;
	};
}
