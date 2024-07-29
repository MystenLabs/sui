// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import type { SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress } from '@mysten/sui/utils';

import { BalanceManagerContract } from './transactions/balanceManager.js';
import { DeepBookContract } from './transactions/deepbook.js';
import { DeepBookAdminContract } from './transactions/deepbookAdmin.js';
import { FlashLoanContract } from './transactions/flashLoans.js';
import { GovernanceContract } from './transactions/governance.js';
import type { BalanceManager, Environment } from './types/index.js';
import { DEEP_SCALAR, DeepBookConfig, FLOAT_SCALAR } from './utils/config.js';
import type { CoinMap, PoolMap } from './utils/constants.js';

/**
 * DeepBookClient class for managing DeepBook operations.
 */
export class DeepBookClient {
	client: SuiClient;
	#config: DeepBookConfig;
	#address: string;
	balanceManager: BalanceManagerContract;
	deepBook: DeepBookContract;
	deepBookAdmin: DeepBookAdminContract;
	flashLoans: FlashLoanContract;
	governance: GovernanceContract;

	/**
	 * @param {SuiClient} client SuiClient instance
	 * @param {string} address Address of the client
	 * @param {Environment} env Environment configuration
	 * @param {Object.<string, BalanceManager>} [balanceManagers] Optional initial BalanceManager map
	 * @param {CoinMap} [coins] Optional initial CoinMap
	 * @param {PoolMap} [pools] Optional initial PoolMap
	 * @param {string} [adminCap] Optional admin capability
	 */
	constructor({
		client,
		address,
		env,
		balanceManagers,
		coins,
		pools,
		adminCap,
	}: {
		client: SuiClient;
		address: string;
		env: Environment;
		balanceManagers?: { [key: string]: BalanceManager };
		coins?: CoinMap;
		pools?: PoolMap;
		adminCap?: string;
	}) {
		this.client = client;
		this.#address = normalizeSuiAddress(address);
		this.#config = new DeepBookConfig({
			address: this.#address,
			env,
			balanceManagers,
			coins,
			pools,
			adminCap,
		});
		this.balanceManager = new BalanceManagerContract(this.#config);
		this.deepBook = new DeepBookContract(this.#config);
		this.deepBookAdmin = new DeepBookAdminContract(this.#config);
		this.flashLoans = new FlashLoanContract(this.#config);
		this.governance = new GovernanceContract(this.#config);
	}

	/**
	 * @description Check the balance of a balance manager for a specific coin
	 * @param {string} managerKey Key of the balance manager
	 * @param {string} coinKey Key of the coin
	 * @returns {Promise<{ coinType: string, balance: number }>} An object with coin type and balance
	 */
	async checkManagerBalance(managerKey: string, coinKey: string) {
		const tx = new Transaction();
		const coin = this.#config.getCoin(coinKey);

		tx.add(this.balanceManager.checkManagerBalance(managerKey, coinKey));
		const res = await this.client.devInspectTransactionBlock({
			sender: this.#address,
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const parsed_balance = bcs.U64.parse(new Uint8Array(bytes));
		const balanceNumber = Number(parsed_balance);
		const adjusted_balance = balanceNumber / coin.scalar;

		return {
			coinType: coin.type,
			balance: adjusted_balance,
		};
	}

	/**
	 * @description Check if a pool is whitelisted
	 * @param {string} poolKey Key of the pool
	 * @returns {Promise<boolean>} Boolean indicating if the pool is whitelisted
	 */
	async whitelisted(poolKey: string) {
		const tx = new Transaction();

		tx.add(this.deepBook.whitelisted(poolKey));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const whitelisted = bcs.Bool.parse(new Uint8Array(bytes));

		return whitelisted;
	}

	/**
	 * @description Get the quote quantity out for a given base quantity
	 * @param {string} poolKey Key of the pool
	 * @param {number} baseQuantity Base quantity to convert
	 * @returns {Promise<{ baseQuantity: number, baseOut: number, quoteOut: number, deepRequired: number }>}
	 * An object with base quantity, base out, quote out, and deep required for the dry run
	 */
	async getQuoteQuantityOut(poolKey: string, baseQuantity: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.getQuoteQuantityOut(poolKey, baseQuantity));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			baseOut: baseOut / baseScalar,
			quoteOut: quoteOut / quoteScalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	/**
	 * @description Get the base quantity out for a given quote quantity
	 * @param {string} poolKey Key of the pool
	 * @param {number} quoteQuantity Quote quantity to convert
	 * @returns {Promise<{ quoteQuantity: number, baseOut: number, quoteOut: number, deepRequired: number }>}
	 * An object with quote quantity, base out, quote out, and deep required for the dry run
	 */
	async getBaseQuantityOut(poolKey: string, quoteQuantity: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.getBaseQuantityOut(poolKey, quoteQuantity));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			quoteQuantity: quoteQuantity,
			baseOut: baseOut / baseScalar,
			quoteOut: quoteOut / quoteScalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	/**
	 * @description Get the output quantities for given base and quote quantities. Only one quantity can be non-zero
	 * @param {string} poolKey Key of the pool
	 * @param {number} baseQuantity Base quantity to convert
	 * @param {number} quoteQuantity Quote quantity to convert
	 * @returns {Promise<{ baseQuantity: number, quoteQuantity: number, baseOut: number, quoteOut: number, deepRequired: number }>}
	 * An object with base quantity, quote quantity, base out, quote out, and deep required for the dry run
	 */
	async getQuantityOut(poolKey: string, baseQuantity: number, quoteQuantity: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.getQuantityOut(poolKey, baseQuantity, quoteQuantity));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			quoteQuantity,
			baseOut: baseOut / baseScalar,
			quoteOut: quoteOut / quoteScalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	/**
	 * @description Get open orders for a balance manager in a pool
	 * @param {string} poolKey Key of the pool
	 * @param {string} managerKey Key of the balance manager
	 * @returns {Promise<Array>} An array of open order IDs
	 */
	async accountOpenOrders(poolKey: string, managerKey: string) {
		const tx = new Transaction();

		tx.add(this.deepBook.accountOpenOrders(poolKey, managerKey));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const order_ids = res.results![0].returnValues![0][0];
		const VecSet = bcs.struct('VecSet', {
			constants: bcs.vector(bcs.U128),
		});

		return VecSet.parse(new Uint8Array(order_ids)).constants;
	}

	/**
	 * @description Get the order information for a specific order in a pool
	 * @param {string} poolKey Key of the pool
	 * @param {string} orderId Order ID
	 * @returns {Promise<Object>} A promise that resolves to an object containing the order information
	 */
	async getOrder(poolKey: string, orderId: string) {
		const tx = new Transaction();

		tx.add(this.deepBook.getOrder(poolKey, orderId));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const ID = bcs.struct('ID', {
			bytes: bcs.Address,
		});
		const OrderDeepPrice = bcs.struct('OrderDeepPrice', {
			asset_is_base: bcs.bool(),
			deep_per_asset: bcs.u64(),
		});
		const Order = bcs.struct('Order', {
			balance_manager_id: ID,
			order_id: bcs.u128(),
			client_order_id: bcs.u64(),
			quantity: bcs.u64(),
			filled_quantity: bcs.u64(),
			fee_is_deep: bcs.bool(),
			order_deep_price: OrderDeepPrice,
			epoch: bcs.u64(),
			status: bcs.u8(),
			expire_timestamp: bcs.u64(),
		});

		const orderInformation = res.results![0].returnValues![0][0];
		return Order.parse(new Uint8Array(orderInformation));
	}

	/**
	 * @description Get level 2 order book specifying range of price
	 * @param {string} poolKey Key of the pool
	 * @param {number} priceLow Lower bound of the price range
	 * @param {number} priceHigh Upper bound of the price range
	 * @param {boolean} isBid Whether to get bid or ask orders
	 * @returns {Promise<{ prices: Array<number>, quantities: Array<number> }>}
	 * An object with arrays of prices and quantities
	 */
	async getLevel2Range(poolKey: string, priceLow: number, priceHigh: number, isBid: boolean) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.add(this.deepBook.getLevel2Range(poolKey, priceLow, priceHigh, isBid));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));

		return {
			prices: parsed_prices.map(
				(price) => (Number(price) / FLOAT_SCALAR / quoteCoin.scalar) * baseCoin.scalar,
			),
			quantities: parsed_quantities.map((price) => Number(price) / baseCoin.scalar),
		};
	}

	/**
	 * @description Get level 2 order book ticks from mid-price for a pool
	 * @param {string} poolKey Key of the pool
	 * @param {number} ticks Number of ticks from mid-price
	 * @returns {Promise<{ bid_prices: Array<number>, bid_quantities: Array<number>, ask_prices: Array<number>, ask_quantities: Array<number> }>}
	 * An object with arrays of prices and quantities
	 */
	async getLevel2TicksFromMid(poolKey: string, ticks: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.add(this.deepBook.getLevel2TicksFromMid(poolKey, ticks));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const bid_prices = res.results![0].returnValues![0][0];
		const bid_parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(bid_prices));
		const bid_quantities = res.results![0].returnValues![1][0];
		const bid_parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(bid_quantities));

		const ask_prices = res.results![0].returnValues![2][0];
		const ask_parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(ask_prices));
		const ask_quantities = res.results![0].returnValues![3][0];
		const ask_parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(ask_quantities));

		return {
			bid_prices: bid_parsed_prices.map(
				(price) => (Number(price) / FLOAT_SCALAR / quoteCoin.scalar) * baseCoin.scalar,
			),
			bid_quantities: bid_parsed_quantities.map((quantity) => Number(quantity) / baseCoin.scalar),
			ask_prices: ask_parsed_prices.map(
				(price) => (Number(price) / FLOAT_SCALAR / quoteCoin.scalar) * baseCoin.scalar,
			),
			ask_quantities: ask_parsed_quantities.map((quantity) => Number(quantity) / baseCoin.scalar),
		};
	}

	/**
	 * @description Get the vault balances for a pool
	 * @param {string} poolKey Key of the pool
	 * @returns {Promise<{ base: number, quote: number, deep: number }>}
	 * An object with base, quote, and deep balances in the vault
	 */
	async vaultBalances(poolKey: string) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.vaultBalances(poolKey));
		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const baseInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			base: baseInVault / baseScalar,
			quote: quoteInVault / quoteScalar,
			deep: deepInVault / DEEP_SCALAR,
		};
	}

	/**
	 * @description Get the pool ID by asset types
	 * @param {string} baseType Type of the base asset
	 * @param {string} quoteType Type of the quote asset
	 * @returns {Promise<string>} The address of the pool
	 */
	async getPoolIdByAssets(baseType: string, quoteType: string) {
		const tx = new Transaction();
		tx.add(this.deepBook.getPoolIdByAssets(baseType, quoteType));

		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const ID = bcs.struct('ID', {
			bytes: bcs.Address,
		});
		const address = ID.parse(new Uint8Array(res.results![0].returnValues![0][0]))['bytes'];

		return address;
	}

	/**
	 * @description Get the mid price for a pool
	 * @param {string} poolKey Key of the pool
	 * @returns {Promise<number>} The mid price
	 */
	async midPrice(poolKey: string) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		tx.add(this.deepBook.midPrice(poolKey));

		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		const res = await this.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress(this.#address),
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const parsed_mid_price = Number(bcs.U64.parse(new Uint8Array(bytes)));
		const adjusted_mid_price =
			(parsed_mid_price * baseCoin.scalar) / quoteCoin.scalar / FLOAT_SCALAR;

		return adjusted_mid_price;
	}
}
