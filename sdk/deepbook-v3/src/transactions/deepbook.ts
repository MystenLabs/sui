// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress, SUI_CLOCK_OBJECT_ID } from '@mysten/sui/utils';

import type { BalanceManager, Pool } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { DEEP_SCALAR, FLOAT_SCALAR, GAS_BUDGET } from '../utils/config.js';

export class DeepBookContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	placeLimitOrder =
		(
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
		) =>
		(tx: Transaction) => {
			tx.setGasBudget(GAS_BUDGET);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
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
		};

	placeMarketOrder =
		(
			pool: Pool,
			balanceManager: BalanceManager,
			clientOrderId: number,
			quantity: number,
			isBid: boolean,
			selfMatchingOption: number,
			payWithDeep: boolean,
		) =>
		(tx: Transaction) => {
			tx.setGasBudget(GAS_BUDGET);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
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
		};

	modifyOrder =
		(pool: Pool, balanceManager: BalanceManager, orderId: number, newQuantity: number) =>
		(tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
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

	cancelOrder =
		(pool: Pool, balanceManager: BalanceManager, orderId: number) => (tx: Transaction) => {
			tx.setGasBudget(GAS_BUDGET);
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
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
		};

	cancelAllOrders = (pool: Pool, balanceManager: BalanceManager) => (tx: Transaction) => {
		tx.setGasBudget(GAS_BUDGET);
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
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
	};

	withdrawSettledAmounts = (pool: Pool, balanceManager: BalanceManager) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::withdraw_settled_amounts`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	addDeepPricePoint = (targetPool: Pool, referencePool: Pool) => (tx: Transaction) => {
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

	claimRebates = (pool: Pool, balanceManager: BalanceManager) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tradeProof = this.#config.balanceManager.generateProof(balanceManager);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::claim_rebates`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	burnDeep = (pool: Pool) => (tx: Transaction) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::burn_deep`,
			arguments: [tx.object(pool.address), tx.object(this.#config.DEEP_TREASURY_ID)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	midPrice = async (pool: Pool) => {
		const tx = new Transaction();
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
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

		return adjusted_mid_price;
	};

	whitelisted = async (pool: Pool) => {
		const tx = new Transaction();
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::whitelisted`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const whitelisted = bcs.Bool.parse(new Uint8Array(bytes));

		return whitelisted;
	};

	getQuoteQuantityOut = async (pool: Pool, baseQuantity: number) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const quoteScalar = quoteCoin.scalar;

		const tx = new Transaction();

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_quote_quantity_out`,
			arguments: [
				tx.object(pool.address),
				tx.pure.u64(baseQuantity * baseCoin.scalar),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		console.log(
			`For ${baseQuantity} base in, you will get ${baseOut / baseCoin.scalar} base, ${quoteOut / quoteScalar} quote, and requires ${deepRequired / DEEP_SCALAR} deep`,
		);
	};

	getBaseQuantityOut = async (pool: Pool, quoteQuantity: number) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const quoteScalar = quoteCoin.scalar;

		const tx = new Transaction();

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_base_quantity_out`,
			arguments: [
				tx.object(pool.address),
				tx.pure.u64(quoteQuantity * quoteScalar),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		console.log(
			`For ${quoteQuantity} quote in, you will get ${baseOut / baseCoin.scalar} base, ${quoteOut / quoteScalar} quote, and requires ${deepRequired / DEEP_SCALAR} deep`,
		);
	};

	getQuantityOut = async (pool: Pool, basequantity: number, quoteQuantity: number) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const quoteScalar = quoteCoin.scalar;

		const tx = new Transaction();

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_quantity_out`,
			arguments: [
				tx.object(pool.address),
				tx.pure.u64(basequantity * baseCoin.scalar),
				tx.pure.u64(quoteQuantity * quoteScalar),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		console.log(
			`For ${basequantity} base and ${quoteQuantity} quote in, you will get ${baseOut / baseCoin.scalar} base, ${quoteOut / quoteScalar} quote, and requires ${deepRequired / DEEP_SCALAR} deep`,
		);
	};

	accountOpenOrders = async (pool: Pool, managerId: string) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		const tx = new Transaction();

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::account_open_orders`,
			arguments: [tx.object(pool.address), tx.pure.id(managerId)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const order_ids = res.results![0].returnValues![0][0];
		const VecSet = bcs.struct('VecSet', {
			constants: bcs.vector(bcs.U128),
		});

		let parsed_order_ids = VecSet.parse(new Uint8Array(order_ids)).constants;

		console.log(parsed_order_ids);
	};

	getLevel2Range = async (pool: Pool, priceLow: number, priceHigh: number, isBid: boolean) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		const tx = new Transaction();

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

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));
		console.log(res.results![0].returnValues![0]);
		console.log(parsed_prices);
		console.log(parsed_quantities);
		return [parsed_prices, parsed_quantities];
	};

	getLevel2TicksFromMid = async (pool: Pool, tickFromMid: number) => {
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const tx = new Transaction();

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_level2_tick_from_mid`,
			arguments: [tx.object(pool.address), tx.pure.u64(tickFromMid)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));
		console.log(res.results![0].returnValues![0]);
		console.log(parsed_prices);
		console.log(parsed_quantities);
		return [parsed_prices, parsed_quantities];
	};

	vaultBalances = async (pool: Pool) => {
		const tx = new Transaction();
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);
		const quoteScalar = quoteCoin.scalar;

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::vault_balances`,
			arguments: [tx.object(pool.address)],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));
		console.log(
			`Base in vault: ${baseInVault / baseCoin.scalar}, Quote in vault: ${quoteInVault / quoteScalar}, Deep in vault: ${deepInVault / DEEP_SCALAR}`,
		);

		return [baseInVault / baseCoin.scalar, quoteInVault / quoteScalar, deepInVault / DEEP_SCALAR];
	};

	getPoolIdByAssets = async (baseType: string, quoteType: string) => {
		const tx = new Transaction();

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::get_pool_id_by_asset`,
			arguments: [tx.object(this.#config.REGISTRY_ID)],
			typeArguments: [baseType, quoteType],
		});

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const ID = bcs.struct('ID', {
			bytes: bcs.Address,
		});
		const address = ID.parse(new Uint8Array(res.results![0].returnValues![0][0]))['bytes'];
		console.log(`Pool ID base ${baseType} and quote ${quoteType} is ${address}`);

		return address;
	};

	swapExactBaseForQuote =
		(
			pool: Pool,
			baseAmount: number,
			baseCoinId: string,
			deepAmount: number,
			deepCoinId: string,
			recepient: string,
		) =>
		(tx: Transaction) => {
			const {
				key: baseCoinKey,
				scalar: baseScalar,
				type: baseType,
			} = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);

			let baseCoin;
			if (baseCoinKey === 'SUI') {
				[baseCoin] = tx.splitCoins(tx.gas, [tx.pure.u64(baseAmount * baseScalar)]);
			} else {
				[baseCoin] = tx.splitCoins(tx.object(baseCoinId), [tx.pure.u64(baseAmount * baseScalar)]);
			}
			const [deepCoin] = tx.splitCoins(tx.object(deepCoinId), [
				tx.pure.u64(deepAmount * DEEP_SCALAR),
			]);
			let [baseOut, quoteOut, deepOut] = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_base_for_quote`,
				arguments: [tx.object(pool.address), baseCoin, deepCoin, tx.object(SUI_CLOCK_OBJECT_ID)],
				typeArguments: [baseType, quoteCoin.type],
			});
			tx.transferObjects([baseOut], recepient);
			tx.transferObjects([quoteOut], recepient);
			tx.transferObjects([deepOut], recepient);
		};

	swapExactQuoteForBase =
		(
			pool: Pool,
			quoteAmount: number,
			quoteCoinId: string,
			deepAmount: number,
			deepCoinId: string,
			recepient: string,
		) =>
		(tx: Transaction) => {
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const {
				key: quoteCoinKey,
				scalar: quoteScalar,
				type: quoteType,
			} = this.#config.getCoin(pool.quoteCoin);

			let quoteCoin;
			if (quoteCoinKey === 'SUI') {
				[quoteCoin] = tx.splitCoins(tx.gas, [tx.pure.u64(quoteAmount * quoteScalar)]);
			} else {
				[quoteCoin] = tx.splitCoins(tx.object(quoteCoinId), [
					tx.pure.u64(quoteAmount * quoteScalar),
				]);
			}
			const [deepCoin] = tx.splitCoins(tx.object(deepCoinId), [
				tx.pure.u64(deepAmount * DEEP_SCALAR),
			]);
			let [baseOut, quoteOut, deepOut] = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::swap_exact_quote_for_base`,
				arguments: [tx.object(pool.address), quoteCoin, deepCoin, tx.object(SUI_CLOCK_OBJECT_ID)],
				typeArguments: [baseCoin.type, quoteType],
			});
			tx.transferObjects([baseOut], recepient);
			tx.transferObjects([quoteOut], recepient);
			tx.transferObjects([deepOut], recepient);
		};
}
