// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { SuiClient } from '@mysten/sui/client';
import type { Signer } from '@mysten/sui/cryptography';
import { Transaction } from '@mysten/sui/transactions';

import { BalanceManagerContract } from '../transactions/balanceManager.js';
import type { Environment } from '../types/index.js';
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
	#coins = testnetCoins;
	#pools = testnetPools;
	#coinIds = new Map<string, string>();
	#signer: Signer;
	client: SuiClient;

	DEEPBOOK_PACKAGE_ID = testnetPackageIds.DEEPBOOK_PACKAGE_ID;
	REGISTRY_ID = testnetPackageIds.REGISTRY_ID;
	DEEP_TREASURY_ID = testnetPackageIds.DEEP_TREASURY_ID;
	adminCap?: string;

	balanceManager: BalanceManagerContract;

	constructor({
		client,
		signer,
		env,
		adminCap,
	}: {
		client: SuiClient;
		signer: Signer;
		env: Environment;
		adminCap?: string;
	}) {
		this.client = client;
		this.#signer = signer;

		this.adminCap = adminCap;

		if (env === 'mainnet') {
			this.#coins = mainnetCoins;
			this.#pools = mainnetPools;
			this.DEEPBOOK_PACKAGE_ID = mainnetPackageIds.DEEPBOOK_PACKAGE_ID;
			this.REGISTRY_ID = mainnetPackageIds.REGISTRY_ID;
			this.DEEP_TREASURY_ID = mainnetPackageIds.DEEP_TREASURY_ID;
		}

		this.balanceManager = new BalanceManagerContract(this);
	}

	async init(merge: boolean) {
		await this.#fetchCoinData(merge);
	}

	async #getOwnedCoin(coinType: string): Promise<string | null> {
		const owner = this.#signer.toSuiAddress();
		const res = await this.client.getCoins({
			owner,
			coinType,
			limit: 1,
		});

		if (res.data.length > 0) {
			return res.data[0].coinObjectId;
		} else {
			return null;
		}
	}

	async #fetchCoinData(merge: boolean) {
		// if merge is true and signer provided, merge all whitelisted coins into one object.
		if (merge) {
			for (const coin of Object.values(this.#coins)) {
				if (coin && coin.key !== 'SUI') {
					await this.#mergeAllCoins(coin.type);
				}
			}
		}

		// fetch all coin object IDs and set them internally.
		for (const coin of Object.values(this.#coins)) {
			if (coin && !this.#coinIds.has(coin.key)) {
				const accountCoin = await this.#getOwnedCoin(coin.type);

				if (accountCoin) {
					this.#coinIds.set(coin.key, accountCoin);
				}
			}
		}
	}

	// Merge all owned coins of a specific type into a single coin.
	async #mergeAllCoins(coinType: string): Promise<void> {
		let moreCoinsToMerge = true;
		while (moreCoinsToMerge) {
			moreCoinsToMerge = await this.#mergeOwnedCoins(coinType);
		}
	}

	// Merge all owned coins of a specific type into a single coin.
	// Returns true if there are more coins to be merged still,
	// false otherwise. Run this function in a while loop until it returns false.
	// A gas coin object ID must be explicitly provided to avoid merging it.
	async #mergeOwnedCoins(coinType: string): Promise<boolean> {
		// store all coin objects
		let coins = [];
		const data = await this.client.getCoins({
			owner: this.#signer.toSuiAddress(),
			coinType,
		});

		if (!data || !data.data) {
			throw new Error(`Failed to fetch coins of type: ${coinType}`);
		}

		coins.push(
			...data.data.map((coin) => ({
				objectId: coin.coinObjectId,
				version: coin.version,
				digest: coin.digest,
			})),
		);

		// no need to merge anymore if there are no coins or just one coin left
		if (coins.length <= 1) {
			return false;
		}

		const baseCoin = coins[0];
		const otherCoins = coins.slice(1);

		if (!baseCoin) {
			throw new Error(`Base coin is undefined for type: ${coinType}`);
		}

		const tx = new Transaction();

		tx.mergeCoins(
			tx.objectRef({
				objectId: baseCoin.objectId,
				version: baseCoin.version,
				digest: baseCoin.digest,
			}),
			otherCoins.map((coin) =>
				tx.objectRef({
					objectId: coin.objectId,
					version: coin.version,
					digest: coin.digest,
				}),
			),
		);

		const res = await this.client.signAndExecuteTransaction({
			transaction: tx,
			signer: this.#signer,
			options: {
				showEffects: true,
			},
		});

		return res.effects?.status.status === 'success';
	}

	// Getters
	getCoin(key: string) {
		const coin = this.#coins[key];
		if (!coin) {
			throw new Error(`Coin not found for key: ${key}`);
		}

		const coinId = this.#coinIds.get(key) ?? null;

		if (coinId) {
			return {
				...coin,
				coinId,
			};
		} else {
			return {
				...coin,
				coinId: null,
			};
		}
	}

	getCoinId(key: string) {
		if (!this.#coinIds.has(key)) {
			throw new Error(`Coin ID not initialized for key: ${key}`);
		}

		return this.#coinIds.get(key)!;
	}

	getPool(key: string) {
		const pool = this.#pools[key];
		if (!pool) {
			throw new Error(`Pool not found for key: ${key}`);
		}

		return pool;
	}
}
