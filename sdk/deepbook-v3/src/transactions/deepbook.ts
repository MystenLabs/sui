// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress, SUI_CLOCK_OBJECT_ID } from '@mysten/sui/utils';

import type { BalanceManager, Pool, SwapParams } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { DEEP_SCALAR, FLOAT_SCALAR, GAS_BUDGET } from '../utils/config.js';

export class DeepBookContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	placeLimitOrder = (
		pool: Pool,
		balanceManager: BalanceManager,
		clientOrderId: number,
		price: number,
		quantity: number,
		isBid: boolean,
		expiration: number | bigint,
		orderType: number,
		selfMatchingOption: number,
		payWithDeep: boolean,
		tx: Transaction = new Transaction(),
	) => {
		tx.setGasBudget(GAS_BUDGET);
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const inputPrice = (price * FLOAT_SCALAR * quoteCoin.scalar) / baseCoin.scalar;
		const inputQuantity = quantity * baseCoin.scalar;

		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

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

		return tx;
	};

	placeMarketOrder = (
		pool: Pool,
		balanceManager: BalanceManager,
		clientOrderId: number,
		quantity: number,
		isBid: boolean,
		selfMatchingOption: number,
		payWithDeep: boolean,
		tx: Transaction = new Transaction(),
	) => {
		tx.setGasBudget(GAS_BUDGET);
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

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

		return tx;
	};

	modifyOrder =
		(pool: Pool, balanceManager: BalanceManager, orderId: number, newQuantity: number) =>
		(tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin.key);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
			const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

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

	cancelOrder = (
		pool: Pool,
		balanceManager: BalanceManager,
		orderId: number,
		tx: Transaction = new Transaction(),
	) => {
		tx.setGasBudget(GAS_BUDGET);
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

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

		return tx;
	};

	cancelAllOrders = (
		pool: Pool,
		balanceManager: BalanceManager,
		tx: Transaction = new Transaction(),
	) => {
		tx.setGasBudget(GAS_BUDGET);
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

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

		return tx;
	};

	withdrawSettledAmounts = (
		pool: Pool,
		balanceManager: BalanceManager,
		tx: Transaction = new Transaction(),
	) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::withdraw_settled_amounts`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	addDeepPricePoint = (
		targetPool: Pool,
		referencePool: Pool,
		tx: Transaction = new Transaction(),
	) => {
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

		return tx;
	};

	claimRebates = (
		pool: Pool,
		balanceManager: BalanceManager,
		tx: Transaction = new Transaction(),
	) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::claim_rebates`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return tx;
	};

	burnDeep = (pool: Pool, tx: Transaction = new Transaction()) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::burn_deep`,
			arguments: [tx.object(pool.address), tx.object(this.#config.DEEP_TREASURY_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return tx;
	};

	midPrice = async (pool: Pool, tx: Transaction = new Transaction()) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const quoteScalar = quoteCoin.scalar;

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::mid_price`,
			arguments: [tx.object(pool.address), tx.object(SUI_CLOCK_OBJECT_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const parsed_mid_price = Number(bcs.U64.parse(new Uint8Array(bytes)));
		const adjusted_mid_price = (parsed_mid_price * baseCoin.scalar) / quoteScalar / FLOAT_SCALAR;

		return {
			transaction: tx,
			adjusted_mid_price,
		};
	};

	whitelisted = (pool: Pool, tx: Transaction = new Transaction()) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::whitelisted`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return tx;
	};

	getQuoteQuantityOut = (pool: Pool, baseQuantity: number, tx: Transaction = new Transaction()) => {
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

		return tx;
	};

	getBaseQuantityOut = (pool: Pool, quoteQuantity: number, tx: Transaction = new Transaction()) => {
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

		return tx;
	};

	getQuantityOut = (
		pool: Pool,
		baseQuantity: number,
		quoteQuantity: number,
		tx: Transaction = new Transaction(),
	) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);
		const quoteScalar = quoteCoin.scalar;

		return tx.moveCall({
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

	accountOpenOrders = (pool: Pool, managerId: string, tx: Transaction = new Transaction()) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::account_open_orders`,
			arguments: [tx.object(pool.address), tx.pure.id(managerId)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	getLevel2Range = (
		pool: Pool,
		priceLow: number,
		priceHigh: number,
		isBid: boolean,
		tx: Transaction = new Transaction(),
	) => {
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

		return tx;
	};

	getLevel2TicksFromMid = (
		pool: Pool,
		tickFromMid: number,
		tx: Transaction = new Transaction(),
	) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_level2_tick_from_mid`,
			arguments: [tx.object(pool.address), tx.pure.u64(tickFromMid)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		return tx;
	};

	vaultBalances = (pool: Pool, tx: Transaction = new Transaction()) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin.key);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin.key);

		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::vault_balances`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	getPoolIdByAssets = (
		baseType: string,
		quoteType: string,
		tx: Transaction = new Transaction(),
	) => {
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_pool_id_by_asset`,
			arguments: [tx.object(this.#config.REGISTRY_ID)],
			typeArguments: [baseType, quoteType],
		});
	};

	swapExactBaseForQuote = (params: SwapParams, tx: Transaction = new Transaction()) => {
		tx.setGasBudget(GAS_BUDGET);
		const { poolKey, amount: baseAmount, deepAmount, deepCoin } = params;

		let pool = this.#config.getPool(poolKey);
		let baseCoinId = this.#config.getCoinId(pool.baseCoin.key);
		let deepCoinId = this.#config.getCoinId('DEEP');
		const baseScalar = pool.baseCoin.scalar;

		let baseCoin;
		if (pool.baseCoin.key === 'SUI') {
			[baseCoin] = tx.splitCoins(tx.gas, [tx.pure.u64(baseAmount * baseScalar)]);
		} else {
			[baseCoin] = tx.splitCoins(tx.object(baseCoinId), [tx.pure.u64(baseAmount * baseScalar)]);
		}
		var deepCoinInput = deepCoin;
		if (!deepCoin) {
			[deepCoinInput] = tx.splitCoins(tx.object(deepCoinId), [
				tx.pure.u64(deepAmount * DEEP_SCALAR),
			]);
		}
		console.log(baseCoinId, deepCoinId);
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_base_for_quote`,
			arguments: [tx.object(pool.address), baseCoin, deepCoinInput, tx.object(SUI_CLOCK_OBJECT_ID)],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});
	};

	swapExactQuoteForBase = (params: SwapParams, tx: Transaction = new Transaction()) => {
		tx.setGasBudget(GAS_BUDGET);
		const { poolKey, amount: quoteAmount, deepAmount, deepCoin } = params;

		let pool = this.#config.getPool(poolKey);
		let quoteCoinId = this.#config.getCoinId(pool.quoteCoin.key);
		let deepCoinId = this.#config.getCoinId('DEEP');
		const quoteScalar = pool.quoteCoin.scalar;

		let quoteCoin;
		if (pool.quoteCoin.key === 'SUI') {
			[quoteCoin] = tx.splitCoins(tx.gas, [tx.pure.u64(quoteAmount * quoteScalar)]);
		} else {
			[quoteCoin] = tx.splitCoins(tx.object(quoteCoinId), [tx.pure.u64(quoteAmount * quoteScalar)]);
		}

		var deepCoinInput = deepCoin;
		if (!deepCoin) {
			[deepCoinInput] = tx.splitCoins(tx.object(deepCoinId), [
				tx.pure.u64(deepAmount * DEEP_SCALAR),
			]);
		}
		console.log(quoteCoin, deepCoinInput);
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_quote_for_base`,
			arguments: [
				tx.object(pool.address),
				quoteCoin,
				deepCoinInput,
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
		});
	};
}
