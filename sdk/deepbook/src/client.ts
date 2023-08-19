// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { OrderArguments, PaginatedEvents, PaginationArguments } from '@mysten/sui.js/client';
import {
	SUI_CLOCK_OBJECT_ID,
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	parseStructTag,
} from '@mysten/sui.js/utils';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import {
	MODULE_CLOB,
	PACKAGE_ID,
	NORMALIZED_SUI_COIN_TYPE,
	CREATION_FEE,
	MODULE_CUSTODIAN,
	ORDER_DEFAULT_EXPIRATION_IN_MS,
} from './utils';
import {
	Level2BookStatusPoint,
	LimitOrderType,
	MarketPrice,
	Order,
	PaginatedPoolSummary,
	PoolSummary,
	SelfMatchingPreventionStyle,
	UserPosition,
	bcs,
} from './types';
import { Coin } from '@mysten/sui.js';

const DUMMY_ADDRESS = normalizeSuiAddress('0x0');

export class DeepBookClient {
	#poolTypeArgsCache: Map<string, string[]> = new Map();
	/**
	 *
	 * @param suiClient connection to fullnode
	 * @param accountCap (optional) only required for wrting operations
	 * @param currentAddress (optional) address of the current user (default: DUMMY_ADDRESS)
	 */
	constructor(
		public suiClient: SuiClient = new SuiClient({ url: getFullnodeUrl('testnet') }),
		public accountCap: string | undefined = undefined,
		public currentAddress: string = DUMMY_ADDRESS,
		private clientOrderId: number = 0,
	) {}

	/**
	 * @param cap set the account cap for interacting with DeepBook
	 */
	async setAccountCap(cap: string) {
		this.accountCap = cap;
	}

	/**
	 * @description: Create pool for trading pair
	 * @param baseAssetType Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param quoteAssetType Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param tickSize Minimal Price Change Accuracy of this pool, eg: 10000000. The number must be an integer float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param lotSize Minimal Lot Change Accuracy of this pool, eg: 10000.
	 */
	createPool(
		baseAssetType: string,
		quoteAssetType: string,
		tickSize: bigint,
		lotSize: bigint,
	): TransactionBlock {
		const txb = new TransactionBlock();
		// create a pool with CREATION_FEE
		const [coin] = txb.splitCoins(txb.gas, [txb.pure(CREATION_FEE)]);
		txb.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::create_pool`,
			arguments: [txb.pure(tickSize), txb.pure(lotSize), coin],
		});
		return txb;
	}

	/**
	 * @description: Create pool for trading pair
	 * @param baseAssetType Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
	 * @param quoteAssetType Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
	 * @param tickSize Minimal Price Change Accuracy of this pool, eg: 10000000. The number must be an interger float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param lotSize Minimal Lot Change Accuracy of this pool, eg: 10000.
	 * @param takerFeeRate Customized taker fee rate, float scaled by `FLOAT_SCALING_FACTOR`, Taker_fee_rate of 0.25% should be 2_500_000 for example
	 * @param makerRebateRate Customized maker rebate rate, float scaled by `FLOAT_SCALING_FACTOR`,  should be less than or equal to the taker_rebate_rate
	 */
	createCustomizedPool(
		baseAssetType: string,
		quoteAssetType: string,
		tickSize: bigint,
		lotSize: bigint,
		takerFeeRate: bigint,
		makerRebateRate: bigint,
	): TransactionBlock {
		const txb = new TransactionBlock();
		// create a pool with CREATION_FEE
		const [coin] = txb.splitCoins(txb.gas, [txb.pure(CREATION_FEE)]);
		txb.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
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
	createAccount(currentAddress: string = this.currentAddress): TransactionBlock {
		const txb = new TransactionBlock();
		let [cap] = txb.moveCall({
			typeArguments: [],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::create_account`,
			arguments: [],
		});
		txb.transferObjects([cap], txb.pure(this.#checkAddress(currentAddress)));
		return txb;
	}

	/**
	 * @description: Create and Transfer custodian account to user
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param accountCap: Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
	 */
	createChildAccountCap(
		currentAddress: string = this.currentAddress,
		accountCap: string | undefined = this.accountCap,
	): TransactionBlock {
		const txb = new TransactionBlock();
		let [childCap] = txb.moveCall({
			typeArguments: [],
			target: `${PACKAGE_ID}::${MODULE_CUSTODIAN}::create_child_account_cap`,
			arguments: [txb.object(this.#checkAccountCap(accountCap))],
		});
		txb.transferObjects([childCap], txb.pure(this.#checkAddress(currentAddress)));
		return txb;
	}

	/**
	 * @description construct transaction block for depositing asset into a pool.
	 * @param poolId the pool id for the deposit
	 * @param coinId the coin used for the deposit. You can omit this argument if you are depositing SUI, in which case
	 * gas coin will be used
	 * @param amount the amount of coin to deposit. If omitted, the entire balance of the coin will be deposited
	 */
	async deposit(
		poolId: string,
		coinId: string | undefined = undefined,
		quantity: bigint | undefined = undefined,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();

		const [baseAsset, quoteAsset] = await this.getPoolTypeArgs(poolId);
		const hasSui =
			baseAsset === NORMALIZED_SUI_COIN_TYPE || quoteAsset === NORMALIZED_SUI_COIN_TYPE;

		if (coinId === undefined && !hasSui) {
			throw new Error('coinId must be specified if neither baseAsset nor quoteAsset is SUI');
		}

		const inputCoin = coinId ? txb.object(coinId) : txb.gas;

		const [coin] = quantity ? txb.splitCoins(inputCoin, [txb.pure(quantity)]) : [inputCoin];

		const coinType = coinId ? await this.#getCoinType(coinId) : NORMALIZED_SUI_COIN_TYPE;
		if (coinType !== baseAsset && coinType !== quoteAsset) {
			throw new Error(
				`coin ${coinId} of ${coinType} type is not a valid asset for pool ${poolId}, which supports ${baseAsset} and ${quoteAsset}`,
			);
		}
		const functionName = coinType === baseAsset ? 'deposit_base' : 'deposit_quote';

		txb.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::${functionName}`,
			arguments: [txb.object(poolId), coin, txb.object(this.#checkAccountCap())],
		});
		return txb;
	}

	/**
	 * @description construct transaction block for withdrawing asset from a pool.
	 * @param poolId the pool id for the withdraw
	 * @param amount the amount of coin to withdraw
	 * @param assetType Base or Quote
	 * @param recipientAddress the address to receive the withdrawn asset. If omitted, `this.currentAddress` will be used. The function
	 * will throw if the `recipientAddress === DUMMY_ADDRESS`
	 */
	async withdraw(
		poolId: string,
		// TODO: implement withdraw all
		quantity: bigint,
		assetType: 'base' | 'quote',
		recipientAddress: string = this.currentAddress,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		const functionName = assetType === 'base' ? 'withdraw_base' : 'withdraw_quote';
		const [withdraw] = txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::${functionName}`,
			arguments: [txb.object(poolId), txb.pure(quantity), txb.object(this.#checkAccountCap())],
		});
		txb.transferObjects([withdraw], txb.pure(this.#checkAddress(recipientAddress)));
		return txb;
	}

	/**
	 * @description: place a limit order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param price: price of the limit order. The number must be an interger float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param quantity: quantity of the limit order in BASE ASSET, eg: 100000000.
	 * @param orderType: bid for buying base with quote, ask for selling base for quote
	 * @param expirationTimestamp: expiration timestamp of the limit order in ms, eg: 1620000000000. If omitted, the order will expire in 1 day
	 * from the time this function is called(not the time the transaction is executed)
	 * @param restriction restrictions on limit orders, explain in doc for more details, eg: 0
	 * @param clientOrderId a client side defined order number for bookkeeping purpose, e.g., "1", "2", etc. If omitted, the sdk will
	 * assign a increasing number starting from 0. But this number might be duplicated if you are using multiple sdk instances
	 * @param selfMatchingPrevention: Options for self-match prevention. Right now only support `CANCEL_OLDEST`
	 */
	async placeLimitOrder(
		poolId: string,
		price: bigint,
		quantity: bigint,
		orderType: 'bid' | 'ask',
		expirationTimestamp: number = Date.now() + ORDER_DEFAULT_EXPIRATION_IN_MS,
		restriction: LimitOrderType = LimitOrderType.NO_RESTRICTION,
		clientOrderId: string | undefined = undefined,
		selfMatchingPrevention: SelfMatchingPreventionStyle = SelfMatchingPreventionStyle.CANCEL_OLDEST,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		const args = [
			txb.object(poolId),
			txb.pure(clientOrderId ?? this.#nextClientOrderId()),
			txb.pure(price),
			txb.pure(quantity),
			txb.pure(selfMatchingPrevention),
			txb.pure(orderType === 'bid'),
			txb.pure(expirationTimestamp),
			txb.pure(restriction),
			txb.object(SUI_CLOCK_OBJECT_ID),
			txb.object(this.#checkAccountCap()),
		];
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::place_limit_order`,
			arguments: args,
		});
		return txb;
	}

	/**
	 * @description: place a market order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param quantity Amount of quote asset to swap in base asset
	 * @param orderType bid for buying base with quote, ask for selling base for quote
	 * @param baseCoin the objectId of the base coin
	 * @param quoteCoin the objectId of the quote coin
	 * @param clientOrderId a client side defined order id for bookkeeping purpose. eg: "1" , "2", ... If omitted, the sdk will
	 * assign a increasing number starting from 0. But this number might be duplicated if you are using multiple sdk instances
	 * @param recipientAddress: address to return the unused amounts, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 */
	async placeMarketOrder(
		poolId: string,
		quantity: bigint,
		orderType: 'bid' | 'ask',
		baseCoin: string | undefined = undefined,
		quoteCoin: string | undefined = undefined,
		clientOrderId: string | undefined = undefined,
		recipientAddress: string = this.currentAddress,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		const [baseAssetType, quoteAssetType] = await this.getPoolTypeArgs(poolId);
		if (!baseCoin && orderType === 'ask') {
			throw new Error('Must specify a valid base coin for an ask order');
		} else if (!quoteCoin && orderType === 'bid') {
			throw new Error('Must specify a valid quote coin for a bid order');
		}
		const emptyCoin = txb.moveCall({
			typeArguments: [baseCoin ? quoteAssetType : baseAssetType],
			target: `0x2::coin::zero`,
			arguments: [],
		});
		const [base_coin_ret, quote_coin_ret] = txb.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::place_market_order`,
			arguments: [
				txb.object(poolId),
				txb.object(this.#checkAccountCap()),
				txb.pure(clientOrderId ?? this.#nextClientOrderId()),
				txb.pure(quantity),
				txb.pure(orderType === 'bid'),
				baseCoin ? txb.object(baseCoin) : emptyCoin,
				quoteCoin ? txb.object(quoteCoin) : emptyCoin,
				txb.object(SUI_CLOCK_OBJECT_ID),
			],
		});
		const recipient = this.#checkAddress(recipientAddress);
		txb.transferObjects([base_coin_ret], txb.pure(recipient));
		txb.transferObjects([quote_coin_ret], txb.pure(recipient));
		return txb;
	}

	/**
	 * @description: swap exact quote for base
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param tokenObjectIn: Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
	 * @param amountIn: amount of token to buy or sell, eg: 10000000.
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param clientOrderId a client side defined order id for bookkeeping purpose, eg: "1" , "2", ... If omitted, the sdk will
	 * assign a increasing number starting from 0. But this number might be duplicated if you are using multiple sdk instances
	 */
	async swapExactQuoteForBase(
		poolId: string,
		tokenObjectIn: string,
		amountIn: bigint,
		currentAddress: string,
		clientOrderId: string | undefined = undefined,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		// in this case, we assume that the tokenIn--tokenOut always exists.
		const [base_coin_ret, quote_coin_ret, _amount] = txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_quote_for_base`,
			arguments: [
				txb.object(poolId),
				txb.pure(clientOrderId ?? this.#nextClientOrderId()),
				txb.object(this.#checkAccountCap()),
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
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param tokenObjectIn Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
	 * @param amountIn amount of token to buy or sell, eg: 10000000
	 * @param currentAddress current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param clientOrderId a client side defined order number for bookkeeping purpose. eg: "1" , "2", ...
	 */
	async swapExactBaseForQuote(
		poolId: string,
		tokenObjectIn: string,
		amountIn: bigint,
		currentAddress: string,
		clientOrderId: string | undefined = undefined,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		const [baseAsset, quoteAsset] = await this.getPoolTypeArgs(poolId);
		// in this case, we assume that the tokenIn--tokenOut always exists.
		const [base_coin_ret, quote_coin_ret, _amount] = txb.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_base_for_quote`,
			arguments: [
				txb.object(poolId),
				txb.pure(clientOrderId ?? this.#nextClientOrderId()),
				txb.object(this.#checkAccountCap()),
				txb.object(String(amountIn)),
				txb.object(tokenObjectIn),
				txb.moveCall({
					typeArguments: [quoteAsset],
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
	 * @description: cancel an order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param orderId orderId of a limit order, you can find them through function query.list_open_orders eg: "0"
	 */
	async cancelOrder(poolId: string, orderId: string): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::cancel_order`,
			arguments: [txb.object(poolId), txb.pure(orderId), txb.object(this.#checkAccountCap())],
		});
		return txb;
	}

	/**
	 * @description: Cancel all limit orders under a certain account capacity
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 */
	async cancelAllOrders(poolId: string): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::cancel_all_orders`,
			arguments: [txb.object(poolId), txb.object(this.#checkAccountCap())],
		});
		return txb;
	}

	/**
	 * @description: batch cancel order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param orderIds array of order ids you want to cancel, you can find your open orders by query.list_open_orders eg: ["0", "1", "2"]
	 */
	async batchCancelOrder(poolId: string, orderIds: string[]): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::batch_cancel_order`,
			arguments: [txb.object(poolId), txb.pure(orderIds), txb.object(this.#checkAccountCap())],
		});
		return txb;
	}

	/**
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param orderIds array of expired order ids to clean, eg: ["0", "1", "2"]
	 * @param orderOwners array of Order owners, should be the owner addresses from the account capacities which placed the orders
	 */
	async cleanUpExpiredOrders(
		poolId: string,
		orderIds: string[],
		orderOwners: string[],
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
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

	/**
	 * @description returns paginated list of pools created in DeepBook by querying for the
	 * `PoolCreated` event. Warning: this method can return incomplete results if the upstream data source
	 * is pruned.
	 */
	async getAllPools(
		input: PaginationArguments<PaginatedEvents['nextCursor']> & OrderArguments,
	): Promise<PaginatedPoolSummary> {
		const resp = await this.suiClient.queryEvents({
			query: { MoveEventType: `${PACKAGE_ID}::${MODULE_CLOB}::PoolCreated` },
			...input,
		});
		const pools = resp.data.map((event) => {
			const rawEvent = event.parsedJson as any;
			return {
				poolId: rawEvent.pool_id as string,
				baseAsset: normalizeStructTag(rawEvent.base_asset.name),
				quoteAsset: normalizeStructTag(rawEvent.quote_asset.name),
			};
		});
		return {
			data: pools,
			nextCursor: resp.nextCursor,
			hasNextPage: resp.hasNextPage,
		};
	}

	/**
	 * @description Fetch metadata for a pool
	 * @param poolId object id of the pool
	 * @returns Metadata for the Pool
	 */
	async getPoolInfo(poolId: string): Promise<PoolSummary> {
		const resp = await this.suiClient.getObject({
			id: poolId,
			options: { showContent: true },
		});
		if (resp?.data?.content?.dataType !== 'moveObject') {
			throw new Error(`pool ${poolId} does not exist`);
		}

		const [baseAsset, quoteAsset] = parseStructTag(resp!.data!.content!.type).typeParams.map((t) =>
			normalizeStructTag(t),
		);

		return {
			poolId,
			baseAsset,
			quoteAsset,
		};
	}

	async getPoolTypeArgs(poolId: string): Promise<string[]> {
		if (!this.#poolTypeArgsCache.has(poolId)) {
			const { baseAsset, quoteAsset } = await this.getPoolInfo(poolId);
			const typeArgs = [baseAsset, quoteAsset];
			this.#poolTypeArgsCache.set(poolId, typeArgs);
		}

		return this.#poolTypeArgsCache.get(poolId)!;
	}

	/**
	 * @description get the order status
	 * @param poolId: the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param orderId the order id, eg: "1"
	 */
	async getOrderStatus(
		poolId: string,
		orderId: string,
		accountCap: string | undefined = this.accountCap,
	): Promise<Order | undefined> {
		const txb = new TransactionBlock();
		const cap = this.#checkAccountCap(accountCap);
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_order_status`,
			arguments: [txb.object(poolId), txb.object(orderId), txb.object(cap)],
		});
		const results = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: txb,
				sender: this.currentAddress,
			})
		).results;

		if (!results) {
			return undefined;
		}

		return bcs.de('Order', Uint8Array.from(results![0].returnValues![0][0]));
	}

	/**
	 * @description: get the base and quote token in custodian account
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param accountCap your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3. If not provided, `this.accountCap` will be used.
	 */
	async getUserPosition(
		poolId: string,
		accountCap: string | undefined = undefined,
	): Promise<UserPosition> {
		const txb = new TransactionBlock();
		const cap = this.#checkAccountCap(accountCap);

		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::account_balance`,
			arguments: [txb.object(normalizeSuiObjectId(poolId)), txb.object(cap)],
		});
		const [availableBaseAmount, lockedBaseAmount, availableQuoteAmount, lockedQuoteAmount] = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: txb,
				sender: this.currentAddress,
			})
		).results![0].returnValues!.map(([bytes, _]) => BigInt(bcs.de('u64', Uint8Array.from(bytes))));
		return {
			availableBaseAmount,
			lockedBaseAmount,
			availableQuoteAmount,
			lockedQuoteAmount,
		};
	}

	/**
	 * @description get the open orders of the current user
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param accountCap your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3. If not provided, `this.accountCap` will be used.
	 */
	async listOpenOrders(
		poolId: string,
		accountCap: string | undefined = undefined,
	): Promise<Order[]> {
		const txb = new TransactionBlock();
		const cap = this.#checkAccountCap(accountCap);

		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::list_open_orders`,
			arguments: [txb.object(poolId), txb.object(cap)],
		});

		const results = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: txb,
				sender: this.currentAddress,
			})
		).results;

		if (!results) {
			return [];
		}

		return bcs.de('vector<Order>', Uint8Array.from(results![0].returnValues![0][0]));
	}

	/**
	 * @description get the market price {bestBidPrice, bestAskPrice}
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 */
	async getMarketPrice(poolId: string): Promise<MarketPrice> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_market_price`,
			arguments: [txb.object(poolId)],
		});
		const resp = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: txb,
				sender: this.currentAddress,
			})
		).results![0].returnValues!.map(([bytes, _]) => {
			const opt = bcs.de('Option<u64>', Uint8Array.from(bytes));
			return 'Some' in opt ? BigInt(opt.Some) : undefined;
		});

		return { bestBidPrice: resp[0], bestAskPrice: resp[1] };
	}

	/**
	 * @description get level2 book status
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param lowerPrice lower price you want to query in the level2 book, eg: 18000000000. The number must be an integer float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param higherPrice higher price you want to query in the level2 book, eg: 20000000000. The number must be an integer float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param isBidSide true: query bid side, false: query ask side
	 */
	async getLevel2BookStatus(
		poolId: string,
		lowerPrice: bigint,
		higherPrice: bigint,
		side: 'bid' | 'ask',
	): Promise<Level2BookStatusPoint[]> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_level2_book_status_${side}_side`,
			arguments: [
				txb.object(poolId),
				txb.pure(String(lowerPrice)),
				txb.pure(String(higherPrice)),
				txb.object(SUI_CLOCK_OBJECT_ID),
			],
		});
		const results = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: txb,
				sender: this.currentAddress,
			})
		).results![0].returnValues!.map(([bytes, _]) =>
			bcs.de('vector<u64>', Uint8Array.from(bytes)).map((s: string) => BigInt(s)),
		);
		return results[0].map((price: bigint, i: number) => ({ price, depth: results[1][i] }));
	}

	#checkAccountCap(accountCap: string | undefined = undefined): string {
		const cap = accountCap ?? this.accountCap;
		if (cap === undefined) {
			throw new Error('accountCap is undefined, please call setAccountCap() first');
		}
		return normalizeSuiObjectId(cap);
	}

	#checkAddress(recipientAddress: string): string {
		if (recipientAddress === DUMMY_ADDRESS) {
			throw new Error('Current address cannot be DUMMY_ADDRESS');
		}
		return normalizeSuiAddress(recipientAddress);
	}

	async #getCoinType(coinId: string) {
		const resp = await this.suiClient.getObject({
			id: coinId,
			options: { showType: true },
		});
		return Coin.getCoinTypeArg(resp);
	}

	#nextClientOrderId() {
		const id = this.clientOrderId;
		this.clientOrderId += 1;
		return id;
	}
}
