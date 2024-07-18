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
import type { Environment } from './types/index.js';
import { DEEP_SCALAR, DeepBookConfig, FLOAT_SCALAR } from './utils/config.js';
import type { CoinMap, PoolMap } from './utils/constants.js';

/**
 * DeepBook Client. If a private key is provided, then all transactions
 * will be signed with that key. Otherwise, the default key will be used.
 * Placing orders requires a balance manager to be set.
 * Client is initialized with default Coins and Pools. To trade on more pools,
 * new coins / pools must be added to the client.
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
	 * @param client SuiClient instance
	 * @param address Address of the client
	 * @param env Environment configuration
	 * @param coins Optional initial CoinMap
	 * @param pools Optional initial PoolMap
	 */
	constructor({
		client,
		address,
		env,
		coins,
		pools,
	}: {
		client: SuiClient;
		address: string;
		env: Environment;
		coins?: CoinMap;
		pools?: PoolMap;
	}) {
		this.client = client;
		this.#address = normalizeSuiAddress(address);
		this.#config = new DeepBookConfig({ address: this.#address, env, coins, pools });
		this.balanceManager = new BalanceManagerContract(this.#config);
		this.deepBook = new DeepBookContract(this.#config);
		this.deepBookAdmin = new DeepBookAdminContract(this.#config);
		this.flashLoans = new FlashLoanContract(this.#config);
		this.governance = new GovernanceContract(this.#config);
	}

	setConfig(config: DeepBookConfig) {
		this.#config = config;
	}

	/**
	 * @description Check the balance of a balance manager for a specific coin
	 * @param managerKey Key of the balance manager
	 * @param coinKey Key of the coin
	 * @returns An object with coin type and balance
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
	 * @param poolKey Key of the pool
	 * @returns Boolean indicating if the pool is whitelisted
	 */
	async whitelisted(poolKey: string) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);

		tx.add(this.deepBook.whitelisted(pool));
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
	 * @param poolKey Key of the pool
	 * @param baseQuantity Base quantity to convert
	 * @returns An object with base quantity, base out, quote out, and deep required for the dry run
	 */
	async getQuoteQuantityOut(poolKey: string, baseQuantity: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.getQuoteQuantityOut(pool, baseQuantity));
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
	 * @param poolKey Key of the pool
	 * @param quoteQuantity Quote quantity to convert
	 * @returns An object with quote quantity, base out, quote out, and deep required for the dry run
	 */
	async getBaseQuantityOut(poolKey: string, quoteQuantity: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.getBaseQuantityOut(pool, quoteQuantity));
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
	 * @param poolKey Key of the pool
	 * @param baseQuantity Base quantity to convert
	 * @param quoteQuantity Quote quantity to convert
	 * @returns An object with base quantity, quote quantity, base out, quote out, and deep required for the dry run
	 */
	async getQuantityOut(poolKey: string, baseQuantity: number, quoteQuantity: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.getQuantityOut(pool, baseQuantity, quoteQuantity));
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
	 * @param poolKey Key of the pool
	 * @param managerKey Key of the balance manager
	 * @returns An array of open order IDs
	 */
	async accountOpenOrders(poolKey: string, managerKey: string) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);

		tx.add(this.deepBook.accountOpenOrders(pool, managerKey));
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
	 * @description Get level 2 order book specifying range of price
	 * @param poolKey Key of the pool
	 * @param priceLow Lower bound of the price range
	 * @param priceHigh Upper bound of the price range
	 * @param isBid Whether to get bid or ask orders
	 * @returns An object with arrays of prices and quantities
	 */
	async getLevel2Range(poolKey: string, priceLow: number, priceHigh: number, isBid: boolean) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.add(this.deepBook.getLevel2Range(pool, priceLow, priceHigh, isBid));
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
			quantities: parsed_quantities.map(
				(price) => (Number(price) / FLOAT_SCALAR / quoteCoin.scalar) * baseCoin.scalar,
			),
		};
	}

	/**
	 * @description Get level 2 order book ticks from mid-price for a pool
	 * @param poolKey Key of the pool
	 * @param ticks Number of ticks from mid-price
	 * @returns An object with arrays of prices and quantities
	 */
	async getLevel2TicksFromMid(poolKey: string, ticks: number) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.add(this.deepBook.getLevel2TicksFromMid(pool, ticks));
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
			bid_quantities: bid_parsed_quantities.map((quantity) => Number(quantity) / FLOAT_SCALAR),
			ask_prices: ask_parsed_prices.map(
				(price) => (Number(price) / FLOAT_SCALAR / quoteCoin.scalar) * baseCoin.scalar,
			),
			ask_quantities: ask_parsed_quantities.map((quantity) => Number(quantity) / FLOAT_SCALAR),
		};
	}

	/**
	 * @description Get the vault balances for a pool
	 * @param poolKey Key of the pool
	 * @returns An object with base, quote, and deep balances in the vault
	 */
	async vaultBalances(poolKey: string) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		const baseScalar = this.#config.getCoin(pool.baseCoin).scalar;
		const quoteScalar = this.#config.getCoin(pool.quoteCoin).scalar;

		tx.add(this.deepBook.vaultBalances(pool));
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
	 * @param baseType Type of the base asset
	 * @param quoteType Type of the quote asset
	 * @returns The address of the pool
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
	 * @param poolKey Key of the pool
	 * @returns The mid price
	 */
	async midPrice(poolKey: string) {
		const tx = new Transaction();
		const pool = this.#config.getPool(poolKey);
		tx.add(this.deepBook.midPrice(pool));

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

	/**
	 * @description Add a balance manager
	 * @param managerKey Key for the balance manager
	 * @param managerId ID of the balance manager
	 * @param tradeCapId Optional tradeCap ID
	 */
	addBalanceManager(managerKey: string, managerId: string, tradeCapId?: string) {
		this.#config.balanceManagers[managerKey] = {
			address: managerId,
			tradeCap: tradeCapId,
		};
		this.balanceManager = new BalanceManagerContract(this.#config);
		this.deepBook = new DeepBookContract(this.#config);
		this.deepBookAdmin = new DeepBookAdminContract(this.#config);
		this.flashLoans = new FlashLoanContract(this.#config);
		this.governance = new GovernanceContract(this.#config);
	}
}
