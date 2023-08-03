// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_CLOCK_OBJECT_ID } from '@mysten/sui.js/utils';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { CREATION_FEE, MODULE_CLOB, MODULE_CUSTODIAN, PACKAGE_ID } from './utils';

/**
 * @description: Create pool for trading pair
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param tickSize Minimal Price Change Accuracy of this pool, eg: 10000000
 * @param lotSize Minimal Lot Change Accuracy of this pool, eg: 10000
 */
export function createPool(
	token1: string,
	token2: string,
	tickSize: number,
	lotSize: number,
): TransactionBlock {
	const txb = new TransactionBlock();
	// create a pool with CREATION_FEE
	const [coin] = txb.splitCoins(txb.gas, [txb.pure(CREATION_FEE)]);
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::create_pool`,
		arguments: [txb.pure(tickSize), txb.pure(lotSize), coin],
	});
	return txb;
}

/**
 * @description: Create pool for trading pair
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param tickSize Minimal Price Change Accuracy of this pool, eg: 10000000
 * @param lotSize Minimal Lot Change Accuracy of this pool, eg: 10000
 * @param takerFeeRate Customized taker fee rate, 10^9 scaling, Taker_fee_rate of 0.25% should be 2_500_000 for example
 * @param makerRebateRate Customized maker rebate rate, 10^9 scaling,  should be less than or equal to the taker_rebate_rate
 */
export function createCustomizedPool(
	token1: string,
	token2: string,
	tickSize: number,
	lotSize: number,
	takerFeeRate: number,
	makerRebateRate: number,
): TransactionBlock {
	const txb = new TransactionBlock();
	// create a pool with CREATION_FEE
	const [coin] = txb.splitCoins(txb.gas, [txb.pure(CREATION_FEE)]);
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::create_customized_pool`,
		arguments: [
			txb.pure(tickSize),
			txb.pure(lotSize),
			txb.pure(takerFeeRate),
			txb.pure(makerRebateRate),
			coin,
		],
	});
	return txb;
}

/**
 * @description: Create and Transfer custodian account to user
 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 */
export function createAccount(currentAddress: string): TransactionBlock {
	const txb = new TransactionBlock();
	let [cap] = txb.moveCall({
		typeArguments: [],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::create_account`,
		arguments: [],
	});
	txb.transferObjects([cap], txb.pure(currentAddress));
	return txb;
}

/**
 * @description: Create and Transfer custodian account to user
 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 * @param accountCap: Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function createChildAccountCap(
	currentAddress: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	let [childCap] = txb.moveCall({
		typeArguments: [],
		target: `${PACKAGE_ID}::${MODULE_CUSTODIAN}::create_child_account_cap`,
		arguments: [txb.object(accountCap)],
	});
	txb.transferObjects([childCap], txb.pure(currentAddress));
	return txb;
}

/**
 * @description: Deposit base asset into custodian account
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param coin Object id of coin to deposit, eg: "0x316467544c7e719384579ac5745c75be5984ca9f004d6c09fd7ca24e4d8a3d14"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function depositBase(
	token1: string,
	token2: string,
	poolId: string,
	coin: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::deposit_base`,
		arguments: [txb.object(poolId), txb.object(coin), txb.object(accountCap)],
	});
	return txb;
}

/**
 * @description: Deposit quote asset into custodian account
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param coin: Object id of coin to deposit, eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function depositQuote(
	token1: string,
	token2: string,
	poolId: string,
	coin: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::deposit_quote`,
		arguments: [txb.object(poolId), txb.object(coin), txb.object(accountCap)],
	});
	return txb;
}

/**
 * @description: Withdraw base asset from custodian account
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param quantity Amount of base asset to withdraw, eg: 10000000
 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function withdrawBase(
	token1: string,
	token2: string,
	poolId: string,
	quantity: number,
	currentAddress: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	const withdraw = txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::withdraw_base`,
		arguments: [txb.object(poolId), txb.pure(quantity), txb.object(accountCap)],
	});
	txb.transferObjects([withdraw], txb.pure(currentAddress));
	return txb;
}

/**
 * @description: Withdraw quote asset from custodian account
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param quantity Amount of base asset to withdraw, eg: 10000000
 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function withdrawQuote(
	token1: string,
	token2: string,
	poolId: string,
	quantity: number,
	currentAddress: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	const withdraw = txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::withdraw_quote`,
		arguments: [txb.object(poolId), txb.pure(quantity), txb.object(accountCap)],
	});
	txb.transferObjects([withdraw], txb.pure(currentAddress));
	return txb;
}

/**
 * @description: swap exact quote for base
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param clientOrderId an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param quantity Amount of quote asset to swap in base asset
 * @param isBid true if the order is bid, false if the order is ask
 * @param baseCoin the objectId of the base coin
 * @param quoteCoin the objectId of the quote coin
 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function placeMarketOrder(
	token1: string,
	token2: string,
	clientOrderId: string,
	poolId: string,
	quantity: number,
	isBid: boolean,
	baseCoin: string,
	quoteCoin: string,
	currentAddress: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	const [base_coin_ret, quote_coin_ret] = txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::place_market_order`,
		arguments: [
			txb.object(poolId),
			txb.object(accountCap),
			txb.pure(clientOrderId),
			txb.pure(quantity),
			txb.pure(isBid),
			txb.object(baseCoin),
			txb.object(quoteCoin),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
	});
	txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
	txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
	return txb;
}

/**
 * @description: swap exact quote for base
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param clientOrderId an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param tokenObjectIn: Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
 * @param amountIn: amount of token to buy or sell, eg: 10000000
 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function swapExactQuoteForBase(
	token1: string,
	token2: string,
	clientOrderId: string,
	poolId: string,
	tokenObjectIn: string,
	amountIn: number,
	currentAddress: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	// in this case, we assume that the tokenIn--tokenOut always exists.
	const [base_coin_ret, quote_coin_ret, _amount] = txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_quote_for_base`,
		arguments: [
			txb.object(poolId),
			txb.pure(clientOrderId),
			txb.object(accountCap),
			txb.object(String(amountIn)),
			txb.object(SUI_CLOCK_OBJECT_ID),
			txb.object(tokenObjectIn),
		],
	});
	txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
	txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
	return txb;
}

/**
 * @description swap exact base for quote
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param clientOrderId an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param tokenObjectIn Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
 * @param amountIn amount of token to buy or sell, eg: 10000000
 * @param currentAddress current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function swapExactBaseForQuote(
	token1: string,
	token2: string,
	clientOrderId: string,
	poolId: string,
	tokenObjectIn: string,
	amountIn: number,
	currentAddress: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	// in this case, we assume that the tokenIn--tokenOut always exists.
	const [base_coin_ret, quote_coin_ret, _amount] = txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_base_for_quote`,
		arguments: [
			txb.object(poolId),
			txb.pure(clientOrderId),
			txb.object(accountCap),
			txb.object(String(amountIn)),
			txb.object(tokenObjectIn),
			txb.moveCall({
				typeArguments: [token2],
				target: `0x2::coin::zero`,
				arguments: [],
			}),
			txb.object(SUI_CLOCK_OBJECT_ID),
		],
	});
	txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
	txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
	return txb;
}

/**
 * @description: place a limit order
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param clientOrderId
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param price: price of the limit order, eg: 180000000
 * @param quantity: quantity of the limit order in BASE ASSET, eg: 100000000
 * @param self_matching_prevention: true for self matching prevention, false for not, eg: true
 * @param isBid: true for buying base with quote, false for selling base for quote
 * @param expireTimestamp: expire timestamp of the limit order in ms, eg: 1620000000000
 * @param restriction restrictions on limit orders, explain in doc for more details, eg: 0
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function placeLimitOrder(
	token1: string,
	token2: string,
	clientOrderId: string,
	poolId: string,
	price: number,
	quantity: number,
	self_matching_prevention: boolean,
	isBid: boolean,
	expireTimestamp: number,
	restriction: number,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	const args = [
		txb.object(poolId),
		txb.pure(clientOrderId),
		txb.pure(Math.floor(price * 1000000000)), // to avoid float number
		txb.pure(quantity),
		txb.pure(self_matching_prevention),
		txb.pure(isBid),
		txb.pure(expireTimestamp),
		txb.pure(restriction),
		txb.object(SUI_CLOCK_OBJECT_ID),
		txb.object(accountCap),
	];
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::place_limit_order`,
		arguments: args,
	});
	return txb;
}

/**
 * @description: cancel an order
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param orderId orderId of a limit order, you can find them through function query.list_open_orders eg: "0"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function cancelOrder(
	token1: string,
	token2: string,
	poolId: string,
	orderId: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::cancel_order`,
		arguments: [txb.object(poolId), txb.pure(orderId), txb.object(accountCap)],
	});
	return txb;
}

/**
 * @description: Cancel all limit orders under a certain account capacity
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function cancelAllOrders(
	token1: string,
	token2: string,
	poolId: string,
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::cancel_all_orders`,
		arguments: [txb.object(poolId), txb.object(accountCap)],
	});
	return txb;
}

/**
 * @description: batch cancel order
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param orderIds array of order ids you want to cancel, you can find your open orders by query.list_open_orders eg: ["0", "1", "2"]
 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
 */
export function batchCancelOrder(
	token1: string,
	token2: string,
	poolId: string,
	orderIds: string[],
	accountCap: string,
): TransactionBlock {
	const txb = new TransactionBlock();
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::batch_cancel_order`,
		arguments: [txb.object(poolId), txb.pure(orderIds), txb.object(accountCap)],
	});
	return txb;
}

/**
 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
 * @param orderIds array of expire order ids to clean, eg: ["0", "1", "2"]
 * @param orderOwners array of Order owners, should be the owner addresses from the account capacities which placed the orders
 */
export function cleanUpExpiredOrders(
	token1: string,
	token2: string,
	poolId: string,
	orderIds: string[],
	orderOwners: string[],
): TransactionBlock {
	const txb = new TransactionBlock();
	txb.moveCall({
		typeArguments: [token1, token2],
		target: `${PACKAGE_ID}::${MODULE_CLOB}::clean_up_expired_orders`,
		arguments: [
			txb.object(poolId),
			txb.object(SUI_CLOCK_OBJECT_ID),
			txb.pure(orderIds),
			txb.pure(orderOwners),
		],
	});
	return txb;
}
