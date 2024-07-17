// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { normalizeSuiAddress } from '@mysten/sui/src/utils/sui-types.js';

import { BalanceManagerContract } from '../transactions/balanceManager.js';
import type { Environment } from '../types/index.js';
import type { CoinMap, PoolMap } from './constants.js';
import {
	mainnetCoins,
	mainnetPackageIds,
	mainnetPools,
	testnetCoins,
	testnetPackageIds,
	testnetPools,
} from './constants.js';

export const FLOAT_SCALAR = 1000000000;
export const POOL_CREATION_FEE = 10000 * 1000000;
export const MAX_TIMESTAMP = 1844674407370955161n;
export const GAS_BUDGET = 0.5 * 500000000; // Adjust based on benchmarking
export const DEEP_SCALAR = 1000000;

export class DeepBookConfig {
	#coins: CoinMap;
	#pools: PoolMap;
	address: string;

	DEEPBOOK_PACKAGE_ID: string;
	REGISTRY_ID: string;
	DEEP_TREASURY_ID: string;
	adminCap?: string;

	balanceManager: BalanceManagerContract;

	constructor({
		env,
		address,
		adminCap,
		coins,
		pools,
	}: {
		env: Environment;
		address: string;
		adminCap?: string;
		coins?: CoinMap;
		pools?: PoolMap;
	}) {
		this.address = normalizeSuiAddress(address);
		this.adminCap = adminCap;

		if (env === 'mainnet') {
			this.#coins = coins || mainnetCoins;
			this.#pools = pools || mainnetPools;
			this.DEEPBOOK_PACKAGE_ID = mainnetPackageIds.DEEPBOOK_PACKAGE_ID;
			this.REGISTRY_ID = mainnetPackageIds.REGISTRY_ID;
			this.DEEP_TREASURY_ID = mainnetPackageIds.DEEP_TREASURY_ID;
		} else {
			this.#coins = coins || testnetCoins;
			this.#pools = pools || testnetPools;
			this.DEEPBOOK_PACKAGE_ID = testnetPackageIds.DEEPBOOK_PACKAGE_ID;
			this.REGISTRY_ID = testnetPackageIds.REGISTRY_ID;
			this.DEEP_TREASURY_ID = testnetPackageIds.DEEP_TREASURY_ID;
		}

		this.balanceManager = new BalanceManagerContract(this);
	}

	setPackageId(packageId: string) {
		this.DEEPBOOK_PACKAGE_ID = packageId;
	}

	setRegistryId(registryId: string) {
		this.REGISTRY_ID = registryId;
	}

	setCoins(coins: CoinMap) {
		this.#coins = coins;
	}

	setPools(pools: PoolMap) {
		this.#pools = pools;
	}

	// Getters
	getCoin(key: string) {
		const coin = this.#coins[key];
		if (!coin) {
			throw new Error(`Coin not found for key: ${key}`);
		}

		return coin;
	}

	getPool(key: string) {
		const pool = this.#pools[key];
		if (!pool) {
			throw new Error(`Pool not found for key: ${key}`);
		}

		return pool;
	}
}
