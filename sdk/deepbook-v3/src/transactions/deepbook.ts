// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { coinWithBalance } from '@mysten/sui/transactions';
import type { Transaction } from '@mysten/sui/transactions';
import { SUI_CLOCK_OBJECT_ID } from '@mysten/sui/utils';

import { OrderType, SelfMatchingOptions } from '../types/index.js';
import type {
	BalanceManager,
	PlaceLimitOrderParams,
	PlaceMarketOrderParams,
	Pool,
	SwapParams,
} from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { DEEP_SCALAR, FLOAT_SCALAR, GAS_BUDGET, MAX_TIMESTAMP } from '../utils/config.js';

export class DeepBookContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	placeLimitOrder = (params: PlaceLimitOrderParams) => (tx: Transaction) => {
		const {
			poolKey,
			balanceManager,
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
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const inputPrice = (price * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar;
		const inputQuantity = quantity * baseCoin.scalar;

		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

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

	placeMarketOrder = (params: PlaceMarketOrderParams) => (tx: Transaction) => {
		const {
			poolKey,
			balanceManager,
			clientOrderId,
			quantity,
			isBid,
			selfMatchingOption = SelfMatchingOptions.SELF_MATCHING_ALLOWED,
			payWithDeep = true,
		} = params;

		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		const pool = this.#config.getPool(poolKey);
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::place_market_order`,
			arguments: [
				tx.object(pool.address),
				tx.object(balanceManager.address),
				tradeProof,
				tx.pure.u64(clientOrderId),
				tx.pure.u8(selfMatchingOption),
				tx.pure.u64(quantity * baseCoin.scalar),
				tx.pure.bool(isBid),
				tx.pure.bool(payWithDeep),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	modifyOrder =
		(pool: Pool, balanceManager: BalanceManager, orderId: number, newQuantity: number) =>
		(tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin.key);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
			const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::modify_order`,
				arguments: [
					tx.object(pool.address),
					tx.object(balanceManager.address),
					tradeProof,
					tx.pure.u128(orderId),
					tx.pure.u64(newQuantity),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	cancelOrder =
		(pool: Pool, balanceManager: BalanceManager, orderId: number) => (tx: Transaction) => {
			tx.setGasBudgetIfNotSet(GAS_BUDGET);
			const baseCoin = this.#config.getCoin(pool.baseCoin.key);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
			const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

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

	cancelAllOrders = (pool: Pool, balanceManager: BalanceManager) => (tx: Transaction) => {
		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

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

	withdrawSettledAmounts = (pool: Pool, balanceManager: BalanceManager) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::withdraw_settled_amounts`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	addDeepPricePoint = (targetPool: Pool, referencePool: Pool) => (tx: Transaction) => {
		const targetBaseCoin = this.#config.getCoin(targetPool.baseCoin.key);
		const targetQuoteCoin = this.#config.getCoin(targetPool.quoteCoin.key);
		const referenceBaseCoin = this.#config.getCoin(referencePool.baseCoin.key);
		const referenceQuoteCoin = this.#config.getCoin(referencePool.quoteCoin.key);
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

	claimRebates = (pool: Pool, balanceManager: BalanceManager) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManager));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::claim_rebates`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	burnDeep = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::burn_deep`,
			arguments: [tx.object(pool.address), tx.object(this.#config.DEEP_TREASURY_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	midPrice = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::mid_price`,
			arguments: [tx.object(pool.address), tx.object(SUI_CLOCK_OBJECT_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	whitelisted = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::whitelisted`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	getQuoteQuantityOut = (pool: Pool, baseQuantity: number) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

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

	getBaseQuantityOut = (pool: Pool, quoteQuantity: number) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
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

	getQuantityOut =
		(pool: Pool, baseQuantity: number, quoteQuantity: number) => (tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin.key);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
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

	accountOpenOrders = (pool: Pool, managerId: string) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::account_open_orders`,
			arguments: [tx.object(pool.address), tx.pure.id(managerId)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	getLevel2Range =
		(pool: Pool, priceLow: number, priceHigh: number, isBid: boolean) => (tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin.key);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_level2_range`,
				arguments: [
					tx.object(pool.address),
					tx.pure.u64((priceLow * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar),
					tx.pure.u64((priceHigh * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar),
					tx.pure.bool(isBid),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	getLevel2TicksFromMid = (pool: Pool, tickFromMid: number) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_level2_tick_from_mid`,
			arguments: [tx.object(pool.address), tx.pure.u64(tickFromMid)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	vaultBalances = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::vault_balances`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	getPoolIdByAssets = (baseType: string, quoteType: string) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_pool_id_by_asset`,
			arguments: [tx.object(this.#config.REGISTRY_ID)],
			typeArguments: [baseType, quoteType],
		});
	};

	swapExactBaseForQuote = (params: SwapParams) => (tx: Transaction) => {
		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		tx.setSenderIfNotSet(this.#config.address);
		const { poolKey, amount: baseAmount, deepAmount, minOut: minQuote } = params;

		let pool = this.#config.getPool(poolKey);
		let deepCoinType = this.#config.getCoin('DEEP').type;
		const baseScalar = pool.baseCoin.scalar;
		const quoteScalar = pool.quoteCoin.scalar;

		const baseCoin =
			params.baseCoin ??
			coinWithBalance({ type: pool.baseCoin.type, balance: baseAmount * baseScalar });

		const deepCoin =
			params.deepCoin ?? coinWithBalance({ type: deepCoinType, balance: deepAmount * DEEP_SCALAR });

		const [baseCoinResult, quoteCoinResult, deepCoinResult] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_base_for_quote`,
			arguments: [
				tx.object(pool.address),
				baseCoin,
				deepCoin,
				tx.pure.u64(quoteScalar * minQuote),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});

		return [baseCoinResult, quoteCoinResult, deepCoinResult] as const;
	};

	swapExactQuoteForBase = (params: SwapParams) => (tx: Transaction) => {
		tx.setGasBudgetIfNotSet(GAS_BUDGET);
		tx.setSenderIfNotSet(this.#config.address);
		const { poolKey, amount: quoteAmount, deepAmount, minOut: minBase } = params;

		let pool = this.#config.getPool(poolKey);
		let deepCoinType = this.#config.getCoin('DEEP').type;
		const baseScalar = pool.baseCoin.scalar;
		const quoteScalar = pool.quoteCoin.scalar;

		const quoteCoin =
			params.quoteCoin ??
			coinWithBalance({ type: pool.quoteCoin.type, balance: quoteAmount * quoteScalar });

		const deepCoin =
			params.deepCoin ?? coinWithBalance({ type: deepCoinType, balance: deepAmount * DEEP_SCALAR });

		const [baseCoinResult, quoteCoinResult, deepCoinResult] = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_quote_for_base`,
			arguments: [
				tx.object(pool.address),
				quoteCoin,
				deepCoin,
				tx.pure.u64(baseScalar * minBase),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});

		return [baseCoinResult, quoteCoinResult, deepCoinResult] as const;
	};
}
