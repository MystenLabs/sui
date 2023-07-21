// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js/transactions';
import { normalizeSuiObjectId } from '@mysten/sui.js/utils';
import { getPoolInfoByRecords } from './utils';
import { PoolInfo, Records } from './utils';
import { defaultGasBudget } from './utils';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/src/client';

export type smartRouteResult = {
	maxSwapTokens: number;
	smartRoute: string[];
};

export type smartRouteResultWithExactPath = {
	txb: TransactionBlock;
	amount: number;
};

export class DeepBook_sdk {
	public provider: SuiClient;
	public currentAddress: string;
	public gasBudget: number;
	public records: Records;

	constructor(
		provider: SuiClient = new SuiClient({ url: getFullnodeUrl('localnet') }),
		currentAddress: string,
		gasBudget: number,
		records: Records,
	) {
		this.provider = provider;
		this.currentAddress = currentAddress;
		this.gasBudget = gasBudget;
		this.records = records;
	}

	/**
	 * @description: Create pool for trading pair
	 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param tickSize Minimal Price Change Accuracy of this pool, eg: 10000000
	 * @param lotSize Minimal Lot Change Accuracy of this pool, eg: 10000
	 */
	public createPool(
		token1: string,
		token2: string,
		tickSize: number,
		lotSize: number,
	): TransactionBlock {
		const txb = new TransactionBlock();
		// 100 sui to create a pool
		const [coin] = txb.splitCoins(txb.gas, [txb.pure(100000000000)]);
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::create_pool`,
			arguments: [txb.pure(`${tickSize}`), txb.pure(`${lotSize}`), coin],
		});
		txb.setGasBudget(this.gasBudget);
		return txb;
	}

	/**
	 * @description: Create and Transfer custodian account to user
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 */
	async createAccount(currentAddress: string): Promise<TransactionBlock | TransactionArgument> {
		const txb = new TransactionBlock();
		let [cap] = txb.moveCall({
			typeArguments: [],
			target: `dee9::clob::create_account`,
			arguments: [],
		});
		txb.transferObjects([cap], txb.pure(currentAddress));
		txb.setSenderIfNotSet(currentAddress);
		txb.setGasBudget(this.gasBudget);
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
	public depositBase(
		token1: string,
		token2: string,
		poolId: string,
		coin: string,
		accountCap: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::deposit_base`,
			arguments: [txb.object(`${poolId}`), txb.object(coin), txb.object(`${accountCap}`)],
		});
		txb.setGasBudget(this.gasBudget);
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
	public depositQuote(
		token1: string,
		token2: string,
		poolId: string,
		coin: string,
		accountCap: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::deposit_quote`,
			arguments: [txb.object(`${poolId}`), txb.object(coin), txb.object(`${accountCap}`)],
		});
		txb.setGasBudget(this.gasBudget);
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
	public async withdrawBase(
		token1: string,
		token2: string,
		poolId: string,
		quantity: number,
		currentAddress: string,
		accountCap: string | TransactionArgument,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		const withdraw = txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::withdraw_base`,
			arguments: [txb.object(`${poolId}`), txb.pure(quantity), txb.object(`${accountCap}`)],
		});
		txb.transferObjects([withdraw], txb.pure(currentAddress));
		txb.setGasBudget(this.gasBudget);
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
	public withdrawQuote(
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
			target: `dee9::clob::withdraw_quote`,
			arguments: [txb.object(`${poolId}`), txb.pure(quantity), txb.object(`${accountCap}`)],
		});
		txb.transferObjects([withdraw], txb.pure(currentAddress));
		txb.setGasBudget(this.gasBudget);
		return txb;
	}

	/**
	 * @description: swap exact quote for base
	 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param tokenObjectIn: Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
	 * @param amountIn: amount of token to buy or sell, eg: 10000000
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 */
	public swap_exact_quote_for_base(
		token1: string,
		token2: string,
		poolId: string,
		tokenObjectIn: string,
		amountIn: number,
		currentAddress: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		// in this case, we assume that the tokenIn--tokenOut always exists.
		// eslint-disable-next-line @typescript-eslint/no-unused-vars
		const [base_coin_ret, quote_coin_ret, _amount] = txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::swap_exact_quote_for_base`,
			arguments: [
				txb.object(String(poolId)),
				txb.object(String(amountIn)),
				txb.object(normalizeSuiObjectId('0x6')),
				txb.object(tokenObjectIn),
			],
		});
		txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
		txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
		txb.setSenderIfNotSet(currentAddress);
		txb.setGasBudget(this.gasBudget);
		return txb;
	}

	/**
	 * @description: swap exact base for quote
	 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param treasury treasury of the quote coin, in the selling case, we will mint a zero quote coin to receive the quote coin from the pool. eg: "0x0a11d301013759e79cb5f89a8bb29c3f9a9bb5be6dec2ddba48ea4b39abc5b5a"
	 * @param tokenObjectIn: Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
	 * @param amountIn: amount of token to buy or sell, eg: 10000000
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 */
	public swap_exact_base_for_quote(
		token1: string,
		token2: string,
		poolId: string,
		treasury: string,
		tokenObjectIn: string,
		amountIn: number,
		currentAddress: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		// in this case, we assume that the tokenIn--tokenOut always exists.
		// eslint-disable-next-line @typescript-eslint/no-unused-vars
		const [base_coin_ret, quote_coin_ret, _amount] = txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::swap_exact_base_for_quote`,
			arguments: [
				txb.object(String(poolId)),
				txb.object(String(amountIn)),
				txb.object(tokenObjectIn),
				// we have to mint a zero amount of token2 in the treasury, only with this object can we pass it into the function.
				this.mint(token2, 0, treasury, txb),
				txb.object(normalizeSuiObjectId('0x6')),
			],
		});
		txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
		txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
		txb.setSenderIfNotSet(currentAddress);
		txb.setGasBudget(this.gasBudget);
		return txb;
	}

	/**
	 * @description: place a limit order
	 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param price: price of the limit order, eg: 180000000
	 * @param quantity: quantity of the limit order in BASE ASSET, eg: 100000000
	 * @param isBid: true for buying base with quote, false for selling base for quote
	 * @param expireTimestamp: expire timestamp of the limit order in ms, eg: 1620000000000
	 * @param restriction restrictions on limit orders, explain in doc for more details, eg: 0
	 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
	 */
	public placeLimitOrder(
		token1: string,
		token2: string,
		poolId: string,
		price: number,
		quantity: number,
		isBid: boolean,
		expireTimestamp: number,
		restriction: number,
		accountCap: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		const args = [
			txb.object(String(poolId)),
			txb.pure(Math.floor(price * 1000000000)), // to avoid float number
			txb.pure(quantity),
			txb.pure(isBid),
			txb.pure(expireTimestamp),
			txb.pure(restriction),
			txb.object(normalizeSuiObjectId('0x6')),
			txb.object(accountCap),
		];
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::place_limit_order`,
			arguments: args,
		});
		txb.setGasBudget(this.gasBudget);
		return txb;
	}

	/**
	 * @description: cancel an order
	 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param orderId orderId of a limit order, you can find them through function query.list_open_orders eg: "0"
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
	 */
	public cancelOrder(
		token1: string,
		token2: string,
		poolId: string,
		orderId: string,
		_currentAddress: string,
		accountCap: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::cancel_order`,
			arguments: [txb.object(poolId), txb.pure(orderId), txb.object(accountCap)],
		});
		txb.setGasBudget(this.gasBudget);
		return txb;
	}

	/**
	 * @description: Cancel all limit orders under a certain account capacity
	 * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
	 */
	public async cancelAllOrders(
		token1: string,
		token2: string,
		poolId: string,
		accountCap: string,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::cancel_all_orders`,
			arguments: [txb.object(poolId), txb.object(accountCap)],
		});
		txb.setGasBudget(this.gasBudget);
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
	public batchCancelOrder(
		token1: string,
		token2: string,
		poolId: string,
		orderIds: string[],
		accountCap: string,
	): TransactionBlock {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `dee9::clob::batch_cancel_order`,
			arguments: [txb.object(String(poolId)), txb.pure(orderIds), txb.object(accountCap)],
		});
		txb.setGasBudget(defaultGasBudget);
		return txb;
	}

	/**
	 * @param tokenInObject the tokenObject you want to swap
	 * @param tokenOut the token you want to swap to
	 * @param amountIn the amount of token you want to swap
	 * @param isBid true for bid, false for ask
	 */
	public async findBestRoute(
		tokenInObject: string,
		tokenOut: string,
		amountIn: number,
		isBid: boolean,
		treasury: string,
	): Promise<smartRouteResult> {
		// const tokenTypeIn: string = convertToTokenType(tokenIn, this.records);
		// should get the tokenTypeIn from tokenInObject
		const tokenInfo = await this.provider.getObject({
			id: tokenInObject,
			options: {
				showType: true,
			},
		});
		if (!tokenInfo?.data?.type) {
			throw new Error(`token ${tokenInObject} not found`);
		}
		const tokenTypeIn = tokenInfo.data.type.split('<')[1].split('>')[0];
		const paths: string[][] = this.dfs(tokenTypeIn, tokenOut, this.records);
		let maxSwapTokens = 0;
		let smartRoute: string[] = [];
		for (const path of paths) {
			const smartRouteResultWithExactPath = await this.placeMarketOrderWithSmartRouting(
				tokenInObject,
				tokenOut,
				isBid,
				amountIn,
				this.currentAddress,
				path,
				treasury,
			);
			if (smartRouteResultWithExactPath && smartRouteResultWithExactPath.amount > maxSwapTokens) {
				maxSwapTokens = smartRouteResultWithExactPath.amount;
				smartRoute = path;
			}
		}
		return { maxSwapTokens, smartRoute };
	}

	/**
	 * @param tokenInObject the tokenObject you want to swap
	 * @param tokenTypeOut the token type you want to swap to
	 * @param isBid true for bid, false for ask
	 * @param amountIn the amount of token you want to swap: eg, 1000000
	 * @param currentAddress your own address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param path the path you want to swap through, for example, you have found that the best route is wbtc --> usdt --> weth, then the path should be ["0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::wbtc::WBTC", "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT", "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH"]
	 */
	public async placeMarketOrderWithSmartRouting(
		tokenInObject: string,
		tokenTypeOut: string,
		isBid: boolean,
		amountIn: number,
		currentAddress: string,
		path: string[],
		treasury: string,
	): Promise<smartRouteResultWithExactPath | undefined> {
		const txb = new TransactionBlock();
		const tokenIn = txb.object(tokenInObject);
		txb.setGasBudget(this.gasBudget);
		txb.setSenderIfNotSet(currentAddress);
		let i = 0;
		let base_coin_ret: TransactionArgument;
		let quote_coin_ret: TransactionArgument;
		let amount: TransactionArgument;
		let lastBid: boolean;
		while (path[i]) {
			const nextPath = path[i + 1] ? path[i + 1] : tokenTypeOut;
			const poolInfo: PoolInfo = getPoolInfoByRecords(path[i], nextPath, this.records);
			let _isBid, _tokenIn, _tokenOut, _amount;
			if (i === 0) {
				if (!isBid) {
					_isBid = false;
					_tokenIn = tokenIn;
					_tokenOut = this.mint(nextPath, 0, treasury, txb);
					_amount = txb.object(String(amountIn));
				} else {
					_isBid = true;
					// _tokenIn = this.mint(txb, nextPath, 0)
					_tokenOut = tokenIn;
					_amount = txb.object(String(amountIn));
				}
			} else {
				if (!isBid) {
					txb.transferObjects(
						// @ts-ignore
						[lastBid ? quote_coin_ret : base_coin_ret],
						txb.pure(currentAddress),
					);
					_isBid = false;
					// @ts-ignore
					_tokenIn = lastBid ? base_coin_ret : quote_coin_ret;
					_tokenOut = this.mint(nextPath, 0, treasury, txb);
					// @ts-ignore
					_amount = amount;
				} else {
					txb.transferObjects(
						// @ts-ignore
						[lastBid ? quote_coin_ret : base_coin_ret],
						txb.pure(currentAddress),
					);
					_isBid = true;
					// _tokenIn = this.mint(txb, nextPath, 0)
					// @ts-ignore
					_tokenOut = lastBid ? base_coin_ret : quote_coin_ret;
					// @ts-ignore
					_amount = amount;
				}
			}
			lastBid = _isBid;
			// in this moveCall we will change to swap_exact_base_for_quote
			// if isBid, we will use swap_exact_quote_for_base
			// is !isBid, we will use swap_exact_base_for_quote
			if (_isBid) {
				// here swap_exact_quote_for_base
				[base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
					typeArguments: [isBid ? nextPath : path[i], isBid ? path[i] : nextPath],
					target: `dee9::clob::swap_exact_quote_for_base`,
					arguments: [
						txb.object(String(poolInfo.clob)),
						_amount,
						txb.object(normalizeSuiObjectId('0x6')),
						_tokenOut,
					],
				});
			} else {
				// here swap_exact_base_for_quote
				[base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
					typeArguments: [isBid ? nextPath : path[i], isBid ? path[i] : nextPath],
					target: `dee9::clob::swap_exact_base_for_quote`,
					arguments: [
						txb.object(String(poolInfo.clob)),
						_amount,
						// @ts-ignore
						_tokenIn,
						_tokenOut,
						txb.object(normalizeSuiObjectId('0x6')),
					],
				});
			}
			if (nextPath === tokenTypeOut) {
				txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
				txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
				break;
			} else {
				i += 1;
			}
		}
		const r = await this.provider.dryRunTransactionBlock({
			transactionBlock: await txb.build({
				provider: this.provider,
			}),
		});
		if (r.effects.status.status === 'success') {
			for (const ele of r.balanceChanges) {
				if (ele.coinType === tokenTypeOut) {
					return {
						txb: txb,
						amount: Number(ele.amount),
					};
				}
			}
		}
		return;
	}

	/**
	 * @param token the token type you want to mint, eg: "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::wbtc::WBTC"
	 * @param quantity the quantity you want to mint, eg: 2000000000
	 * @param treasury the treasury object id, eg: "0x765c7040f06527df0f76d5a38ceaae67c70311c90c266acf15e39f17e0e4ed61"
	 * @param txb TransactionBlock
	 */
	protected mint(
		token: string,
		quantity: number,
		treasury: string,
		txb: TransactionBlock,
	): TransactionArgument {
		return txb.moveCall({
			typeArguments: [token],
			target: `0x2::coin::mint`,
			arguments: [txb.pure(String(treasury)), txb.pure(String(quantity))],
		});
	}

	/**
	 * @param tokenTypeIn the token type you want to swap with
	 * @param tokenTypeOut the token type you want to swap to
	 * @param records the pool records
	 * @param path the path you want to swap through, in the first step, this path is [], then it will be a recursive function
	 * @param depth the depth of the dfs, it is default to 2, which means, there will be a max of two steps of swap(say A-->B--C), but you can change it as you want lol
	 * @param res the result of the dfs, in the first step, this res is [], then it will be a recursive function
	 */
	private dfs(
		tokenTypeIn: string,
		tokenTypeOut: string,
		records: Records,
		path: string[] = [],
		depth: number = 2,
		res: string[][] = [],
	) {
		// first updates the records
		if (depth < 0) {
			return res;
		}
		depth = depth - 1;
		if (tokenTypeIn === tokenTypeOut) {
			res.push(path);
			return [path];
		}
		// find children of tokenIn
		let children: Set<string> = new Set();
		for (const record of records.pools) {
			if (String((record as any).type).indexOf(tokenTypeIn.substring(2)) > -1) {
				String((record as any).type)
					.split(',')
					.forEach((token: string) => {
						if (token.indexOf('clob') !== -1) {
							token = token.split('<')[1];
						} else {
							token = token.split('>')[0].substring(1);
						}
						if (token !== tokenTypeIn && path.indexOf(token) === -1) {
							children.add(token);
						}
					});
			}
		}

		children.forEach((child: string) => {
			const result = this.dfs(child, tokenTypeOut, records, [...path, tokenTypeIn], depth, res);

			return result;
		});

		return res;
	}
}
