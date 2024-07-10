// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress, SUI_CLOCK_OBJECT_ID } from '@mysten/sui/utils';

import type { BalanceManager, Pool } from '../types/index.js';
import { CoinKey } from '../types/index.js';
import {
	DEEP_SCALAR,
	DEEP_TREASURY_ID,
	DEEPBOOK_PACKAGE_ID,
	FLOAT_SCALAR,
	GAS_BUDGET,
	REGISTRY_ID,
} from '../utils/config.js';
import { generateProof } from './balanceManager.js';

let env = process.env.ENVIRONMENT;
if (!env) {
	env = 'testnet';
}
const client = new SuiClient({
	url: getFullnodeUrl(env as 'mainnet' | 'testnet' | 'devnet' | 'localnet'),
});

export const placeLimitOrder = (
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
	txb: Transaction,
) => {
	txb.setGasBudget(GAS_BUDGET);
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;
	const inputPrice = (price * FLOAT_SCALAR * quoteScalar) / baseScalar;
	const inputQuantity = quantity * baseScalar;

	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::place_limit_order`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.u64(clientOrderId),
			txb.pure.u8(orderType),
			txb.pure.u8(selfMatchingOption),
			txb.pure.u64(inputPrice),
			txb.pure.u64(inputQuantity),
			txb.pure.bool(isBid),
			txb.pure.bool(payWithDeep),
			txb.pure.u64(expiration),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const placeMarketOrder = (
	pool: Pool,
	balanceManager: BalanceManager,
	clientOrderId: number,
	quantity: number,
	isBid: boolean,
	selfMatchingOption: number,
	payWithDeep: boolean,
	txb: Transaction,
) => {
	txb.setGasBudget(GAS_BUDGET);
	const baseScalar = pool.baseCoin.scalar;
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::place_market_order`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.u64(clientOrderId),
			txb.pure.u8(selfMatchingOption),
			txb.pure.u64(quantity * baseScalar),
			txb.pure.bool(isBid),
			txb.pure.bool(payWithDeep),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const modifyOrder = (
	pool: Pool,
	balanceManager: BalanceManager,
	orderId: number,
	newQuantity: number,
	txb: Transaction,
) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::modify_order`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.u128(orderId),
			txb.pure.u64(newQuantity),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const cancelOrder = (
	pool: Pool,
	balanceManager: BalanceManager,
	orderId: number,
	txb: Transaction,
) => {
	txb.setGasBudget(GAS_BUDGET);
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::cancel_order`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.u128(orderId),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const cancelAllOrders = (pool: Pool, balanceManager: BalanceManager, txb: Transaction) => {
	txb.setGasBudget(GAS_BUDGET);
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::cancel_all_orders`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const withdrawSettledAmounts = (
	pool: Pool,
	balanceManager: BalanceManager,
	txb: Transaction,
) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::withdraw_settled_amounts`,
		arguments: [txb.object(pool.address), txb.object(balanceManager.address), tradeProof],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const addDeepPricePoint = (targetPool: Pool, referencePool: Pool, txb: Transaction) => {
	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::add_deep_price_point`,
		arguments: [
			txb.object(targetPool.address),
			txb.object(referencePool.address),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [
			targetPool.baseCoin.type,
			targetPool.quoteCoin.type,
			referencePool.baseCoin.type,
			referencePool.quoteCoin.type,
		],
	});
};

export const claimRebates = (pool: Pool, balanceManager: BalanceManager, txb: Transaction) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::claim_rebates`,
		arguments: [txb.object(pool.address), txb.object(balanceManager.address), tradeProof],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const burnDeep = (pool: Pool, txb: Transaction) => {
	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::burn_deep`,
		arguments: [txb.object(pool.address), txb.object(DEEP_TREASURY_ID)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const midPrice = async (pool: Pool, txb: Transaction) => {
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::mid_price`,
		arguments: [txb.object(pool.address), txb.object(SUI_CLOCK_OBJECT_ID)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const bytes = res.results![0].returnValues![0][0];
	const parsed_mid_price = Number(bcs.U64.parse(new Uint8Array(bytes)));
	const adjusted_mid_price = (parsed_mid_price * baseScalar) / quoteScalar / FLOAT_SCALAR;

	return adjusted_mid_price;
};

export const whitelisted = async (pool: Pool, txb: Transaction) => {
	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::whitelisted`,
		arguments: [txb.object(pool.address)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const bytes = res.results![0].returnValues![0][0];
	const whitelisted = bcs.Bool.parse(new Uint8Array(bytes));

	return whitelisted;
};

export const getQuoteQuantityOut = async (pool: Pool, baseQuantity: number, txb: Transaction) => {
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::get_quote_quantity_out`,
		arguments: [
			txb.object(pool.address),
			txb.pure.u64(baseQuantity * baseScalar),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
	const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
	const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

	console.log(
		`For ${baseQuantity} base in, you will get ${baseOut / baseScalar} base, ${quoteOut / quoteScalar} quote, and requires ${deepRequired / DEEP_SCALAR} deep`,
	);
};

export const getBaseQuantityOut = async (pool: Pool, quoteQuantity: number, txb: Transaction) => {
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::get_base_quantity_out`,
		arguments: [
			txb.object(pool.address),
			txb.pure.u64(quoteQuantity * quoteScalar),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
	const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
	const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

	console.log(
		`For ${quoteQuantity} quote in, you will get ${baseOut / baseScalar} base, ${quoteOut / quoteScalar} quote, and requires ${deepRequired / DEEP_SCALAR} deep`,
	);
};

export const getQuantityOut = async (
	pool: Pool,
	basequantity: number,
	quoteQuantity: number,
	txb: Transaction,
) => {
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::get_quantity_out`,
		arguments: [
			txb.object(pool.address),
			txb.pure.u64(basequantity * baseScalar),
			txb.pure.u64(quoteQuantity * quoteScalar),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
	const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
	const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

	console.log(
		`For ${basequantity} base and ${quoteQuantity} quote in, you will get ${baseOut / baseScalar} base, ${quoteOut / quoteScalar} quote, and requires ${deepRequired / DEEP_SCALAR} deep`,
	);
};

export const accountOpenOrders = async (pool: Pool, managerId: string, txb: Transaction) => {
	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::account_open_orders`,
		arguments: [txb.object(pool.address), txb.pure.id(managerId)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});

	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const order_ids = res.results![0].returnValues![0][0];
	const VecSet = bcs.struct('VecSet', {
		constants: bcs.vector(bcs.U128),
	});

	let parsed_order_ids = VecSet.parse(new Uint8Array(order_ids)).constants;

	console.log(parsed_order_ids);
};

export const getLevel2Range = async (
	pool: Pool,
	priceLow: number,
	priceHigh: number,
	isBid: boolean,
	txb: Transaction,
) => {
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::get_level2_range`,
		arguments: [
			txb.object(pool.address),
			txb.pure.u64((priceLow * FLOAT_SCALAR * quoteScalar) / baseScalar),
			txb.pure.u64((priceHigh * FLOAT_SCALAR * quoteScalar) / baseScalar),
			txb.pure.bool(isBid),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});

	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
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

export const getLevel2TicksFromMid = async (pool: Pool, tickFromMid: number, txb: Transaction) => {
	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::get_level2_tick_from_mid`,
		arguments: [txb.object(pool.address), txb.pure.u64(tickFromMid)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});

	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
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

export const vaultBalances = async (pool: Pool, txb: Transaction) => {
	const baseScalar = pool.baseCoin.scalar;
	const quoteScalar = pool.quoteCoin.scalar;

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::vault_balances`,
		arguments: [txb.object(pool.address)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});

	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const baseInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
	const quoteInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
	const deepInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));
	console.log(
		`Base in vault: ${baseInVault / baseScalar}, Quote in vault: ${quoteInVault / quoteScalar}, Deep in vault: ${deepInVault / DEEP_SCALAR}`,
	);

	return [baseInVault / baseScalar, quoteInVault / quoteScalar, deepInVault / DEEP_SCALAR];
};

export const getPoolIdByAssets = async (baseType: string, quoteType: string, txb: Transaction) => {
	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::get_pool_id_by_asset`,
		arguments: [txb.object(REGISTRY_ID)],
		typeArguments: [baseType, quoteType],
	});

	const res = await client.devInspectTransactionBlock({
		sender: normalizeSuiAddress('0xa'),
		transactionBlock: txb,
	});

	const ID = bcs.struct('ID', {
		bytes: bcs.Address,
	});
	const address = ID.parse(new Uint8Array(res.results![0].returnValues![0][0]))['bytes'];
	console.log(`Pool ID base ${baseType} and quote ${quoteType} is ${address}`);

	return address;
};

export const swapExactBaseForQuote = (
	pool: Pool,
	baseAmount: number,
	baseCoinId: string,
	deepAmount: number,
	deepCoinId: string,
	recepient: string,
	txb: Transaction,
) => {
	const baseScalar = pool.baseCoin.scalar;

	let baseCoin;
	if (pool.baseCoin.key === CoinKey.SUI) {
		[baseCoin] = txb.splitCoins(txb.gas, [txb.pure.u64(baseAmount * baseScalar)]);
	} else {
		[baseCoin] = txb.splitCoins(txb.object(baseCoinId), [txb.pure.u64(baseAmount * baseScalar)]);
	}
	const [deepCoin] = txb.splitCoins(txb.object(deepCoinId), [
		txb.pure.u64(deepAmount * DEEP_SCALAR),
	]);
	let [baseOut, quoteOut, deepOut] = txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::swap_exact_base_for_quote`,
		arguments: [txb.object(pool.address), baseCoin, deepCoin, txb.object(SUI_CLOCK_OBJECT_ID)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	txb.transferObjects([baseOut], recepient);
	txb.transferObjects([quoteOut], recepient);
	txb.transferObjects([deepOut], recepient);
};

export const swapExactQuoteForBase = (
	pool: Pool,
	quoteAmount: number,
	quoteCoinId: string,
	deepAmount: number,
	deepCoinId: string,
	recepient: string,
	txb: Transaction,
) => {
	const quoteScalar = pool.quoteCoin.scalar;

	let quoteCoin;
	if (pool.quoteCoin.key === CoinKey.SUI) {
		[quoteCoin] = txb.splitCoins(txb.gas, [txb.pure.u64(quoteAmount * quoteScalar)]);
	} else {
		[quoteCoin] = txb.splitCoins(txb.object(quoteCoinId), [
			txb.pure.u64(quoteAmount * quoteScalar),
		]);
	}
	const [deepCoin] = txb.splitCoins(txb.object(deepCoinId), [
		txb.pure.u64(deepAmount * DEEP_SCALAR),
	]);
	let [baseOut, quoteOut, deepOut] = txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::swap_exact_quote_for_base`,
		arguments: [txb.object(pool.address), quoteCoin, deepCoin, txb.object(SUI_CLOCK_OBJECT_ID)],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
	txb.transferObjects([baseOut], recepient);
	txb.transferObjects([quoteOut], recepient);
	txb.transferObjects([deepOut], recepient);
};
