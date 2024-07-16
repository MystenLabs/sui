// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { coinWithBalance } from '@mysten/sui/transactions';
import type { Transaction } from '@mysten/sui/transactions';

import type { CreatePoolAdminParams, Pool } from '../types/index.js';
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

	createPoolAdmin = (params: CreatePoolAdminParams) => (tx: Transaction) => {
		tx.setSenderIfNotSet(this.#config.address);
		const { baseCoinKey, quoteCoinKey, tickSize, lotSize, minSize, whitelisted, stablePool } =
			params;
		const baseCoin = this.#config.getCoin(baseCoinKey);
		const quoteCoin = this.#config.getCoin(quoteCoinKey);
		const deepCoinType = this.#config.getCoin('DEEP').type;

		const creationFee = coinWithBalance({ type: deepCoinType, balance: POOL_CREATION_FEE });
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
	};

	unregisterPoolAdmin = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::unregister_pool_admin`,
			arguments: [tx.object(this.#config.REGISTRY_ID), tx.object(this.#adminCap())],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	updateAllowedVersions = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::update_allowed_versions`,
			arguments: [
				tx.object(pool.address),
				tx.object(this.#config.REGISTRY_ID),
				tx.object(this.#adminCap()),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	enableVersion = (version: number) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::regsitry::enable_version`,
			arguments: [
				tx.object(this.#config.REGISTRY_ID),
				tx.pure.u64(version),
				tx.object(this.#adminCap()),
			],
		});
	};

	disableVersion = (version: number) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::regsitry::enable_version`,
			arguments: [
				tx.object(this.#config.REGISTRY_ID),
				tx.pure.u64(version),
				tx.object(this.#adminCap()),
			],
		});
	};
}
