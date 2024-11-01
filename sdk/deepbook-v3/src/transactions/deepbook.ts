// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { coinWithBalance } from '@mysten/sui/transactions';
import type { Transaction } from '@mysten/sui/transactions';
import { SUI_CLOCK_OBJECT_ID } from '@mysten/sui/utils';

import { OrderType, SelfMatchingOptions } from '../types/index.js';
import type { PlaceLimitOrderParams, PlaceMarketOrderParams, SwapParams } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { DEEP_SCALAR, FLOAT_SCALAR, GAS_BUDGET, MAX_TIMESTAMP } from '../utils/config.js';

/**
 * DeepBookContract class for managing DeepBook operations.
 */
export class DeepBookContract {
	#config: DeepBookConfig;

	/**
	 * @param {DeepBookConfig} config Configuration for DeepBookContract
	 */
	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	/**
	 * @description Place a limit order
	 * @param {PlaceLimitOrderParams} params Parameters for placing a limit order
	 * @returns A function that takes a Transaction object
	 */
	placeLimitOrder = (params: PlaceLimitOrderParams) => (tx: Transaction) => {
		const {
			poolKey,
			balanceManagerKey,
			clientOrderId,
			price,
			quantity,
			isBid,
			expiration = MAX_TIMESTAMP,
			orderType = OrderType.NO_RESTRICTION,
			selfMatchingOption = SelfMatchingOptions.SELF_MATCHING_ALLOWED,
			payWithDeep = true,
		} = params;

		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const inputPrice = Math.round((price * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar);
		const inputQuantity = Math.round(quantity * baseCoin.scalar);

		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::place_limit_order`,
			arguments: [
				tx.object(pool.address),
				tx.object(balanceManager.address),
				tradeProof,
				tx.pure.u64(clientOrderId),
				tx.pure.u8(orderType),
				tx.pure.u8(selfMatchingOption),
				tx.pure.u64(inputPrice),
				tx.pure.u64(inputQuantity),
				tx.pure.bool(isBid),
				tx.pure.bool(payWithDeep),
				tx.pure.u64(expiration),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Place a market order
	 * @param {PlaceMarketOrderParams} params Parameters for placing a market order
	 * @returns A function that takes a Transaction object
	 */
	placeMarketOrder = (params: PlaceMarketOrderParams) => (tx: Transaction) => {
		const {
			poolKey,
			balanceManagerKey,
			clientOrderId,
			quantity,
			isBid,
			selfMatchingOption = SelfMatchingOptions.SELF_MATCHING_ALLOWED,
			payWithDeep = true,
		} = params;

		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));
		const inputQuantity = Math.round(quantity * baseCoin.scalar);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::place_market_order`,
			arguments: [
				tx.object(pool.address),
				tx.object(balanceManager.address),
				tradeProof,
				tx.pure.u64(clientOrderId),
				tx.pure.u8(selfMatchingOption),
				tx.pure.u64(inputQuantity),
				tx.pure.bool(isBid),
				tx.pure.bool(payWithDeep),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Modify an existing order
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @param {string} orderId Order ID to modify
	 * @param {number} newQuantity New quantity for the order
	 * @returns A function that takes a Transaction object
	 */
	modifyOrder =
		(poolKey: string, balanceManagerKey: string, orderId: string, newQuantity: number) =>
		(tx: Transaction) => {
			const pool = this.#config.getPool(poolKey);
			const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
			const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));
			const inputQuantity = Math.round(newQuantity * baseCoin.scalar);

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::modify_order`,
				arguments: [
					tx.object(pool.address),
					tx.object(balanceManager.address),
					tradeProof,
					tx.pure.u128(orderId),
					tx.pure.u64(inputQuantity),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	/**
	 * @description Cancel an existing order
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @param {string} orderId Order ID to cancel
	 * @returns A function that takes a Transaction object
	 */
	cancelOrder =
		(poolKey: string, balanceManagerKey: string, orderId: string) => (tx: Transaction) => {
			tx.setGasBudgetIfNotSet(GAS_BUDGET);
			const pool = this.#config.getPool(poolKey);
			const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
			const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::cancel_order`,
				arguments: [
					tx.object(pool.address),
					tx.object(balanceManager.address),
					tradeProof,
					tx.pure.u128(orderId),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	/**
	 * @description Cancel all open orders for a balance manager
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	cancelAllOrders = (poolKey: string, balanceManagerKey: string) => (tx: Transaction) => {
		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::cancel_all_orders`,
			arguments: [
				tx.object(pool.address),
				tx.object(balanceManager.address),
				tradeProof,
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Withdraw settled amounts for a balance manager
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	withdrawSettledAmounts = (poolKey: string, balanceManagerKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::withdraw_settled_amounts`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Add a deep price point for a target pool using a reference pool
	 * @param {string} targetPoolKey The key to identify the target pool
	 * @param {string} referencePoolKey The key to identify the reference pool
	 * @returns A function that takes a Transaction object
	 */
	addDeepPricePoint = (targetPoolKey: string, referencePoolKey: string) => (tx: Transaction) => {
		const targetPool = this.#config.getPool(targetPoolKey);
		const referencePool = this.#config.getPool(referencePoolKey);
		const targetBaseCoin = this.#config.getCoin(targetPool.baseCoin);
		const targetQuoteCoin = this.#config.getCoin(targetPool.quoteCoin);
		const referenceBaseCoin = this.#config.getCoin(referencePool.baseCoin);
		const referenceQuoteCoin = this.#config.getCoin(referencePool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::add_deep_price_point`,
			arguments: [
				tx.object(targetPool.address),
				tx.object(referencePool.address),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [
				targetBaseCoin.type,
				targetQuoteCoin.type,
				referenceBaseCoin.type,
				referenceQuoteCoin.type,
			],
		});
	};

	/**
	 * @description Claim rebates for a balance manager
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	claimRebates = (poolKey: string, balanceManagerKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::claim_rebates`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Gets an order
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} orderId Order ID to get
	 * @returns A function that takes a Transaction object
	 */
	getOrder = (poolKey: string, orderId: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_order`,
			arguments: [tx.object(pool.address), tx.pure.u128(orderId)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Burn DEEP tokens from the pool
	 * @param {string} poolKey The key to identify the pool
	 * @returns A function that takes a Transaction object
	 */
	burnDeep = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::burn_deep`,
			arguments: [tx.object(pool.address), tx.object(this.#config.DEEP_TREASURY_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the mid price for a pool
	 * @param {string} poolKey The key to identify the pool
	 * @returns A function that takes a Transaction object
	 */
	midPrice = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::mid_price`,
			arguments: [tx.object(pool.address), tx.object(SUI_CLOCK_OBJECT_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Check if a pool is whitelisted
	 * @param {string} poolKey The key to identify the pool
	 * @returns A function that takes a Transaction object
	 */
	whitelisted = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::whitelisted`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the quote quantity out for a given base quantity in
	 * @param {string} poolKey The key to identify the pool
	 * @param {number} baseQuantity Base quantity to convert
	 * @returns A function that takes a Transaction object
	 */
	getQuoteQuantityOut = (poolKey: string, baseQuantity: number) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_quote_quantity_out`,
			arguments: [
				tx.object(pool.address),
				tx.pure.u64(baseQuantity * baseCoin.scalar),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the base quantity out for a given quote quantity in
	 * @param {string} poolKey The key to identify the pool
	 * @param {number} quoteQuantity Quote quantity to convert
	 * @returns A function that takes a Transaction object
	 */
	getBaseQuantityOut = (poolKey: string, quoteQuantity: number) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const quoteScalar = quoteCoin.scalar;

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_base_quantity_out`,
			arguments: [
				tx.object(pool.address),
				tx.pure.u64(quoteQuantity * quoteScalar),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the quantity out for a given base or quote quantity
	 * @param {string} poolKey The key to identify the pool
	 * @param {number} baseQuantity Base quantity to convert
	 * @param {number} quoteQuantity Quote quantity to convert
	 * @returns A function that takes a Transaction object
	 */
	getQuantityOut =
		(poolKey: string, baseQuantity: number, quoteQuantity: number) => (tx: Transaction) => {
			const pool = this.#config.getPool(poolKey);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
			const quoteScalar = quoteCoin.scalar;

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_quantity_out`,
				arguments: [
					tx.object(pool.address),
					tx.pure.u64(baseQuantity * baseCoin.scalar),
					tx.pure.u64(quoteQuantity * quoteScalar),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	/**
	 * @description Get open orders for a balance manager in a pool
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} managerKey Key of the balance manager
	 * @returns A function that takes a Transaction object
	 */
	accountOpenOrders = (poolKey: string, managerKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const manager = this.#config.getBalanceManager(managerKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::account_open_orders`,
			arguments: [tx.object(pool.address), tx.object(manager.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get level 2 order book specifying range of price
	 * @param {string} poolKey The key to identify the pool
	 * @param {number} priceLow Lower bound of the price range
	 * @param {number} priceHigh Upper bound of the price range
	 * @param {boolean} isBid Whether to get bid or ask orders
	 * @returns A function that takes a Transaction object
	 */
	getLevel2Range =
		(poolKey: string, priceLow: number, priceHigh: number, isBid: boolean) => (tx: Transaction) => {
			const pool = this.#config.getPool(poolKey);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_level2_range`,
				arguments: [
					tx.object(pool.address),
					tx.pure.u64((priceLow * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar),
					tx.pure.u64((priceHigh * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar),
					tx.pure.bool(isBid),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	/**
	 * @description Get level 2 order book ticks from mid-price for a pool
	 * @param {string} poolKey The key to identify the pool
	 * @param {number} tickFromMid Number of ticks from mid-price
	 * @returns A function that takes a Transaction object
	 */
	getLevel2TicksFromMid = (poolKey: string, tickFromMid: number) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_level2_ticks_from_mid`,
			arguments: [
				tx.object(pool.address),
				tx.pure.u64(tickFromMid),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the vault balances for a pool
	 * @param {string} poolKey The key to identify the pool
	 * @returns A function that takes a Transaction object
	 */
	vaultBalances = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::vault_balances`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the pool ID by asset types
	 * @param {string} baseType Type of the base asset
	 * @param {string} quoteType Type of the quote asset
	 * @returns A function that takes a Transaction object
	 */
	getPoolIdByAssets = (baseType: string, quoteType: string) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_pool_id_by_asset`,
			arguments: [tx.object(this.#config.REGISTRY_ID)],
			typeArguments: [baseType, quoteType],
		});
	};

	/**
	 * @description Swap exact base amount for quote amount
	 * @param {SwapParams} params Parameters for the swap
	 * @returns A function that takes a Transaction object
	 */
	swapExactBaseForQuote = (params: SwapParams) => (tx: Transaction) => {
		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		tx.setSenderIfNotSet(this.#config.address);

		if (params.quoteCoin) {
			throw new Error('quoteCoin is not accepted for swapping base asset');
		}
		const { poolKey, amount: baseAmount, deepAmount, minOut: minQuote } = params;

		let pool = this.#config.getPool(poolKey);
		let deepCoinType = this.#config.getCoin('DEEP').type;
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		const baseCoinInput =
			params.baseCoin ??
			coinWithBalance({ type: baseCoin.type, balance: Math.round(baseAmount * baseCoin.scalar) });

		const deepCoin =
			params.deepCoin ??
			coinWithBalance({ type: deepCoinType, balance: Math.round(deepAmount * DEEP_SCALAR) });

		const minQuoteInput = Math.round(minQuote * quoteCoin.scalar);

		const [baseCoinResult, quoteCoinResult, deepCoinResult] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_base_for_quote`,
			arguments: [
				tx.object(pool.address),
				baseCoinInput,
				deepCoin,
				tx.pure.u64(minQuoteInput),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return [baseCoinResult, quoteCoinResult, deepCoinResult] as const;
	};

	/**
	 * @description Swap exact quote amount for base amount
	 * @param {SwapParams} params Parameters for the swap
	 * @returns A function that takes a Transaction object
	 */
	swapExactQuoteForBase = (params: SwapParams) => (tx: Transaction) => {
		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		tx.setSenderIfNotSet(this.#config.address);

		if (params.baseCoin) {
			throw new Error('baseCoin is not accepted for swapping quote asset');
		}
		const { poolKey, amount: quoteAmount, deepAmount, minOut: minBase } = params;

		let pool = this.#config.getPool(poolKey);
		let deepCoinType = this.#config.getCoin('DEEP').type;
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		const quoteCoinInput =
			params.quoteCoin ??
			coinWithBalance({
				type: quoteCoin.type,
				balance: Math.round(quoteAmount * quoteCoin.scalar),
			});

		const deepCoin =
			params.deepCoin ??
			coinWithBalance({ type: deepCoinType, balance: Math.round(deepAmount * DEEP_SCALAR) });

		const minBaseInput = Math.round(minBase * baseCoin.scalar);

		const [baseCoinResult, quoteCoinResult, deepCoinResult] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_quote_for_base`,
			arguments: [
				tx.object(pool.address),
				quoteCoinInput,
				deepCoin,
				tx.pure.u64(minBaseInput),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return [baseCoinResult, quoteCoinResult, deepCoinResult] as const;
	};

	/**
	 * @description Get the trade parameters for a given pool, including taker fee, maker fee, and stake required.
	 * @param {string} poolKey Key of the pool
	 * @returns A function that takes a Transaction object
	 */
	poolTradeParams = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::pool_trade_params`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the book parameters for a given pool, including tick size, lot size, and min size.
	 * @param {string} poolKey Key of the pool
	 * @returns A function that takes a Transaction object
	 */
	poolBookParams = (poolKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::pool_book_params`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the account information for a given pool and balance manager
	 * @param {string} poolKey Key of the pool
	 * @param {string} managerKey The key of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	account = (poolKey: string, managerKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const managerId = this.#config.getBalanceManager(managerKey).address;

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::account`,
			arguments: [tx.object(pool.address), tx.object(managerId)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Get the locked balance for a given pool and balance manager
	 * @param {string} poolKey Key of the pool
	 * @param {string} managerKey The key of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	lockedBalance = (poolKey: string, managerKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const managerId = this.#config.getBalanceManager(managerKey).address;

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::locked_balance`,
			arguments: [tx.object(pool.address), tx.object(managerId)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};
}
