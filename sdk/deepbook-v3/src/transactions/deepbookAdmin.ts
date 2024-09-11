// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Transaction } from '@mysten/sui/transactions';

import type { CreatePoolAdminParams } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { FLOAT_SCALAR } from '../utils/config.js';

/**
 * DeepBookAdminContract class for managing admin actions.
 */
export class DeepBookAdminContract {
	#config: DeepBookConfig;

	/**
	 * @param {DeepBookConfig} config Configuration for DeepBookAdminContract
	 */
	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	/**
	 * @returns The admin capability required for admin operations
	 * @throws Error if the admin capability is not set
	 */
	#adminCap() {
		const adminCap = this.#config.adminCap;
		if (!adminCap) {
			throw new Error('ADMIN_CAP environment variable not set');
		}
		return adminCap;
	}

	/**
	 * @description Create a new pool as admin
	 * @param {CreatePoolAdminParams} params Parameters for creating pool as admin
	 * @returns A function that takes a Transaction object
	 */
	createPoolAdmin = (params: CreatePoolAdminParams) => (tx: Transaction) => {
		tx.setSenderIfNotSet(this.#config.address);
		const { baseCoinKey, quoteCoinKey, tickSize, lotSize, minSize, whitelisted, stablePool } =
			params;
		const baseCoin = this.#config.getCoin(baseCoinKey);
		const quoteCoin = this.#config.getCoin(quoteCoinKey);

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
				tx.pure.bool(whitelisted),
				tx.pure.bool(stablePool),
				tx.object(this.#adminCap()),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Unregister a pool as admin
	 * @param {string} poolKey The key of the pool to be unregistered by admin
	 * @returns A function that takes a Transaction object
	 */
	unregisterPoolAdmin = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::unregister_pool_admin`,
			arguments: [
				tx.object(pool.address),
				tx.object(this.#config.REGISTRY_ID),
				tx.object(this.#adminCap()),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Update the allowed versions for a pool
	 * @param {string} poolKey The key of the pool to be updated
	 * @returns A function that takes a Transaction object
	 */
	updateAllowedVersions = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
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

	/**
	 * @description Enable a specific version
	 * @param {number} version The version to be enabled
	 * @returns A function that takes a Transaction object
	 */
	enableVersion = (version: number) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::registry::enable_version`,
			arguments: [
				tx.object(this.#config.REGISTRY_ID),
				tx.pure.u64(version),
				tx.object(this.#adminCap()),
			],
		});
	};

	/**
	 * @description Disable a specific version
	 * @param {number} version The version to be disabled
	 * @returns A function that takes a Transaction object
	 */
	disableVersion = (version: number) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::registry::disable_version`,
			arguments: [
				tx.object(this.#config.REGISTRY_ID),
				tx.pure.u64(version),
				tx.object(this.#adminCap()),
			],
		});
	};

	/**
	 * @description Sets the treasury address where pool creation fees will be sent
	 * @param {string} treasuryAddress The treasury address
	 * @returns A function that takes a Transaction object
	 */
	setTreasuryAddress = (treasuryAddress: string) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::registry::set_treasury_address`,
			arguments: [
				tx.object(this.#config.REGISTRY_ID),
				tx.pure.address(treasuryAddress),
				tx.object(this.#adminCap()),
			],
		});
	};
}
