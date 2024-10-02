// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import type { OrderArguments, PaginatedEvents, PaginationArguments } from '@mysten/sui/client';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { Argument, TransactionObjectInput, TransactionResult } from '@mysten/sui/transactions';
import { Transaction } from '@mysten/sui/transactions';
import {
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	parseStructTag,
	SUI_CLOCK_OBJECT_ID,
} from '@mysten/sui/utils';

import { BcsOrder } from './types/bcs.js';
import type {
	Level2BookStatusPoint,
	MarketPrice,
	Order,
	PaginatedPoolSummary,
	PoolSummary,
	UserPosition,
} from './types/index.js';
import { LimitOrderType, SelfMatchingPreventionStyle } from './types/index.js';
import {
	CREATION_FEE,
	MODULE_CLOB,
	MODULE_CUSTODIAN,
	NORMALIZED_SUI_COIN_TYPE,
	ORDER_DEFAULT_EXPIRATION_IN_MS,
	PACKAGE_ID,
} from './utils/index.js';

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
	): Transaction {
		const tx = new Transaction();
		// create a pool with CREATION_FEE
		const [coin] = tx.splitCoins(tx.gas, [CREATION_FEE]);
		tx.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::create_pool`,
			arguments: [tx.pure.u64(tickSize), tx.pure.u64(lotSize), coin],
		});
		return tx;
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
	): Transaction {
		const tx = new Transaction();
		// create a pool with CREATION_FEE
		const [coin] = tx.splitCoins(tx.gas, [CREATION_FEE]);
		tx.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::create_customized_pool`,
			arguments: [
				tx.pure.u64(tickSize),
				tx.pure.u64(lotSize),
				tx.pure.u64(takerFeeRate),
				tx.pure.u64(makerRebateRate),
				coin,
			],
		});
		return tx;
	}

	/**
	 * @description: Create Account Cap
	 * @param tx
	 */
	createAccountCap(tx: Transaction) {
		let [cap] = tx.moveCall({
			typeArguments: [],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::create_account`,
			arguments: [],
		});
		return cap;
	}

	/**
	 * @description: Create and Transfer custodian account to user
	 * @param currentAddress current address of the user
	 * @param tx
	 */
	createAccount(
		currentAddress: string = this.currentAddress,
		tx: Transaction = new Transaction(),
	): Transaction {
		const cap = this.createAccountCap(tx);
		tx.transferObjects([cap], this.#checkAddress(currentAddress));
		return tx;
	}

	/**
	 * @description: Create and Transfer custodian account to user
	 * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param accountCap: Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
	 */
	createChildAccountCap(
		currentAddress: string = this.currentAddress,
		accountCap: string | undefined = this.accountCap,
	): Transaction {
		const tx = new Transaction();
		let [childCap] = tx.moveCall({
			typeArguments: [],
			target: `${PACKAGE_ID}::${MODULE_CUSTODIAN}::create_child_account_cap`,
			arguments: [tx.object(this.#checkAccountCap(accountCap))],
		});
		tx.transferObjects([childCap], this.#checkAddress(currentAddress));
		return tx;
	}

	/**
	 * @description construct transaction for depositing asset into a pool.
	 * @param poolId the pool id for the deposit
	 * @param coinId the coin used for the deposit. You can omit this argument if you are depositing SUI, in which case
	 * gas coin will be used
	 * @param amount the amount of coin to deposit. If omitted, the entire balance of the coin will be deposited
	 */
	async deposit(
		poolId: string,
		coinId: string | undefined = undefined,
		quantity: bigint | undefined = undefined,
	): Promise<Transaction> {
		const tx = new Transaction();

		const [baseAsset, quoteAsset] = await this.getPoolTypeArgs(poolId);
		const hasSui =
			baseAsset === NORMALIZED_SUI_COIN_TYPE || quoteAsset === NORMALIZED_SUI_COIN_TYPE;

		if (coinId === undefined && !hasSui) {
			throw new Error('coinId must be specified if neither baseAsset nor quoteAsset is SUI');
		}

		const inputCoin = coinId ? tx.object(coinId) : tx.gas;

		const [coin] = quantity ? tx.splitCoins(inputCoin, [quantity]) : [inputCoin];

		const coinType = coinId ? await this.getCoinType(coinId) : NORMALIZED_SUI_COIN_TYPE;
		if (coinType !== baseAsset && coinType !== quoteAsset) {
			throw new Error(
				`coin ${coinId} of ${coinType} type is not a valid asset for pool ${poolId}, which supports ${baseAsset} and ${quoteAsset}`,
			);
		}
		const functionName = coinType === baseAsset ? 'deposit_base' : 'deposit_quote';

		tx.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::${functionName}`,
			arguments: [tx.object(poolId), coin, tx.object(this.#checkAccountCap())],
		});
		return tx;
	}

	/**
	 * @description construct transaction for withdrawing asset from a pool.
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
	): Promise<Transaction> {
		const tx = new Transaction();
		const functionName = assetType === 'base' ? 'withdraw_base' : 'withdraw_quote';
		const [withdraw] = tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::${functionName}`,
			arguments: [tx.object(poolId), tx.pure.u64(quantity), tx.object(this.#checkAccountCap())],
		});
		tx.transferObjects([withdraw], this.#checkAddress(recipientAddress));
		return tx;
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
	): Promise<Transaction> {
		const tx = new Transaction();
		const args = [
			tx.object(poolId),
			tx.pure.u64(clientOrderId ?? this.#nextClientOrderId()),
			tx.pure.u64(price),
			tx.pure.u64(quantity),
			tx.pure.u8(selfMatchingPrevention),
			tx.pure.bool(orderType === 'bid'),
			tx.pure.u64(expirationTimestamp),
			tx.pure.u8(restriction),
			tx.object(SUI_CLOCK_OBJECT_ID),
			tx.object(this.#checkAccountCap()),
		];
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::place_limit_order`,
			arguments: args,
		});
		return tx;
	}

	/**
	 * @description: place a market order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param quantity Amount of quote asset to swap in base asset
	 * @param orderType bid for buying base with quote, ask for selling base for quote
	 * @param baseCoin the objectId or the coin object of the base coin
	 * @param quoteCoin the objectId or the coin object of the quote coin
	 * @param clientOrderId a client side defined order id for bookkeeping purpose. eg: "1" , "2", ... If omitted, the sdk will
	 * assign an increasing number starting from 0. But this number might be duplicated if you are using multiple sdk instances
	 * @param accountCap
	 * @param recipientAddress the address to receive the swapped asset. If omitted, `this.currentAddress` will be used. The function
	 * @param tx
	 */
	async placeMarketOrder(
		accountCap: string | Extract<Argument, { $kind: 'NestedResult' }>,
		poolId: string,
		quantity: bigint,
		orderType: 'bid' | 'ask',
		baseCoin: TransactionResult | string | undefined = undefined,
		quoteCoin: TransactionResult | string | undefined = undefined,
		clientOrderId: string | undefined = undefined,
		recipientAddress: string | undefined = this.currentAddress,
		tx: Transaction = new Transaction(),
	): Promise<Transaction> {
		const [baseAssetType, quoteAssetType] = await this.getPoolTypeArgs(poolId);
		if (!baseCoin && orderType === 'ask') {
			throw new Error('Must specify a valid base coin for an ask order');
		} else if (!quoteCoin && orderType === 'bid') {
			throw new Error('Must specify a valid quote coin for a bid order');
		}
		const emptyCoin = tx.moveCall({
			typeArguments: [baseCoin ? quoteAssetType : baseAssetType],
			target: `0x2::coin::zero`,
			arguments: [],
		});

		const [base_coin_ret, quote_coin_ret] = tx.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::place_market_order`,
			arguments: [
				tx.object(poolId),
				typeof accountCap === 'string' ? tx.object(this.#checkAccountCap(accountCap)) : accountCap,
				tx.pure.u64(clientOrderId ?? this.#nextClientOrderId()),
				tx.pure.u64(quantity),
				tx.pure.bool(orderType === 'bid'),
				baseCoin ? tx.object(baseCoin) : emptyCoin,
				quoteCoin ? tx.object(quoteCoin) : emptyCoin,
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
		});
		const recipient = this.#checkAddress(recipientAddress);
		tx.transferObjects([base_coin_ret], recipient);
		tx.transferObjects([quote_coin_ret], recipient);

		return tx;
	}

	/**
	 * @description: swap exact quote for base
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param tokenObjectIn Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
	 * @param amountIn amount of token to buy or sell, eg: 10000000.
	 * @param currentAddress current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param clientOrderId a client side defined order id for bookkeeping purpose, eg: "1" , "2", ... If omitted, the sdk will
	 * assign an increasing number starting from 0. But this number might be duplicated if you are using multiple sdk instances
	 * @param tx
	 */
	async swapExactQuoteForBase(
		poolId: string,
		tokenObjectIn: TransactionObjectInput,
		amountIn: bigint, // quantity of USDC
		currentAddress: string,
		clientOrderId?: string,
		tx: Transaction = new Transaction(),
	): Promise<Transaction> {
		// in this case, we assume that the tokenIn--tokenOut always exists.
		const [base_coin_ret, quote_coin_ret, _amount] = tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_quote_for_base`,
			arguments: [
				tx.object(poolId),
				tx.pure.u64(clientOrderId ?? this.#nextClientOrderId()),
				tx.object(this.#checkAccountCap()),
				tx.pure.u64(String(amountIn)),
				tx.object(SUI_CLOCK_OBJECT_ID),
				tx.object(tokenObjectIn),
			],
		});
		tx.transferObjects([base_coin_ret], currentAddress);
		tx.transferObjects([quote_coin_ret], currentAddress);
		return tx;
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
	): Promise<Transaction> {
		const tx = new Transaction();
		const [baseAsset, quoteAsset] = await this.getPoolTypeArgs(poolId);
		// in this case, we assume that the tokenIn--tokenOut always exists.
		const [base_coin_ret, quote_coin_ret, _amount] = tx.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_base_for_quote`,
			arguments: [
				tx.object(poolId),
				tx.pure.u64(clientOrderId ?? this.#nextClientOrderId()),
				tx.object(this.#checkAccountCap()),
				tx.object(String(amountIn)),
				tx.object(tokenObjectIn),
				tx.moveCall({
					typeArguments: [quoteAsset],
					target: `0x2::coin::zero`,
					arguments: [],
				}),
				tx.object(SUI_CLOCK_OBJECT_ID),
			],
		});
		tx.transferObjects([base_coin_ret], currentAddress);
		tx.transferObjects([quote_coin_ret], currentAddress);
		return tx;
	}

	/**
	 * @description: cancel an order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param orderId orderId of a limit order, you can find them through function query.list_open_orders eg: "0"
	 */
	async cancelOrder(poolId: string, orderId: string): Promise<Transaction> {
		const tx = new Transaction();
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::cancel_order`,
			arguments: [tx.object(poolId), tx.pure.u64(orderId), tx.object(this.#checkAccountCap())],
		});
		return tx;
	}

	/**
	 * @description: Cancel all limit orders under a certain account capacity
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 */
	async cancelAllOrders(poolId: string): Promise<Transaction> {
		const tx = new Transaction();
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::cancel_all_orders`,
			arguments: [tx.object(poolId), tx.object(this.#checkAccountCap())],
		});
		return tx;
	}

	/**
	 * @description: batch cancel order
	 * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
	 * @param orderIds array of order ids you want to cancel, you can find your open orders by query.list_open_orders eg: ["0", "1", "2"]
	 */
	async batchCancelOrder(poolId: string, orderIds: string[]): Promise<Transaction> {
		const tx = new Transaction();
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::batch_cancel_order`,
			arguments: [
				tx.object(poolId),
				bcs.vector(bcs.U64).serialize(orderIds),
				tx.object(this.#checkAccountCap()),
			],
		});
		return tx;
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
	): Promise<Transaction> {
		const tx = new Transaction();
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::clean_up_expired_orders`,
			arguments: [
				tx.object(poolId),
				tx.object(SUI_CLOCK_OBJECT_ID),
				bcs.vector(bcs.U64).serialize(orderIds),
				bcs.vector(bcs.Address).serialize(orderOwners),
			],
		});
		return tx;
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
		const tx = new Transaction();
		const cap = this.#checkAccountCap(accountCap);
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_order_status`,
			arguments: [tx.object(poolId), tx.pure.u64(orderId), tx.object(cap)],
		});
		const results = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: tx,
				sender: this.currentAddress,
			})
		).results;

		if (!results) {
			return undefined;
		}

		return BcsOrder.parse(Uint8Array.from(results![0].returnValues![0][0]));
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
		const tx = new Transaction();
		const cap = this.#checkAccountCap(accountCap);

		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::account_balance`,
			arguments: [tx.object(normalizeSuiObjectId(poolId)), tx.object(cap)],
		});
		const [availableBaseAmount, lockedBaseAmount, availableQuoteAmount, lockedQuoteAmount] = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: tx,
				sender: this.currentAddress,
			})
		).results![0].returnValues!.map(([bytes, _]) => BigInt(bcs.U64.parse(Uint8Array.from(bytes))));
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
		const tx = new Transaction();
		const cap = this.#checkAccountCap(accountCap);

		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::list_open_orders`,
			arguments: [tx.object(poolId), tx.object(cap)],
		});

		const results = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: tx,
				sender: this.currentAddress,
			})
		).results;

		if (!results) {
			return [];
		}

		return bcs.vector(BcsOrder).parse(Uint8Array.from(results![0].returnValues![0][0]));
	}

	/**
	 * @description get the market price {bestBidPrice, bestAskPrice}
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 */
	async getMarketPrice(poolId: string): Promise<MarketPrice> {
		const tx = new Transaction();
		tx.moveCall({
			typeArguments: await this.getPoolTypeArgs(poolId),
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_market_price`,
			arguments: [tx.object(poolId)],
		});
		const resp = (
			await this.suiClient.devInspectTransactionBlock({
				transactionBlock: tx,
				sender: this.currentAddress,
			})
		).results![0].returnValues!.map(([bytes, _]) => {
			const opt = bcs.option(bcs.U64).parse(Uint8Array.from(bytes));
			return opt == null ? undefined : BigInt(opt);
		});

		return { bestBidPrice: resp[0], bestAskPrice: resp[1] };
	}

	/**
	 * @description get level2 book status
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param lowerPrice lower price you want to query in the level2 book, eg: 18000000000. The number must be an integer float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param higherPrice higher price you want to query in the level2 book, eg: 20000000000. The number must be an integer float scaled by `FLOAT_SCALING_FACTOR`.
	 * @param side { 'bid' | 'ask' | 'both' } bid or ask or both sides.
	 */
	async getLevel2BookStatus(
		poolId: string,
		lowerPrice: bigint,
		higherPrice: bigint,
		side: 'bid' | 'ask' | 'both',
	): Promise<Level2BookStatusPoint[] | Level2BookStatusPoint[][]> {
		const tx = new Transaction();
		if (side === 'both') {
			tx.moveCall({
				typeArguments: await this.getPoolTypeArgs(poolId),
				target: `${PACKAGE_ID}::${MODULE_CLOB}::get_level2_book_status_bid_side`,
				arguments: [
					tx.object(poolId),
					tx.pure.u64(lowerPrice),
					tx.pure.u64(higherPrice),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
			});
			tx.moveCall({
				typeArguments: await this.getPoolTypeArgs(poolId),
				target: `${PACKAGE_ID}::${MODULE_CLOB}::get_level2_book_status_ask_side`,
				arguments: [
					tx.object(poolId),
					tx.pure.u64(lowerPrice),
					tx.pure.u64(higherPrice),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
			});
		} else {
			tx.moveCall({
				typeArguments: await this.getPoolTypeArgs(poolId),
				target: `${PACKAGE_ID}::${MODULE_CLOB}::get_level2_book_status_${side}_side`,
				arguments: [
					tx.object(poolId),
					tx.pure.u64(lowerPrice),
					tx.pure.u64(higherPrice),
					tx.object(SUI_CLOCK_OBJECT_ID),
				],
			});
		}

		const results = await this.suiClient.devInspectTransactionBlock({
			transactionBlock: tx,
			sender: this.currentAddress,
		});

		if (side === 'both') {
			const bidSide = results.results![0].returnValues!.map(([bytes, _]) =>
				bcs
					.vector(bcs.U64)
					.parse(Uint8Array.from(bytes))
					.map((s: string) => BigInt(s)),
			);
			const askSide = results.results![1].returnValues!.map(([bytes, _]) =>
				bcs
					.vector(bcs.U64)
					.parse(Uint8Array.from(bytes))
					.map((s: string) => BigInt(s)),
			);
			return [
				bidSide[0].map((price: bigint, i: number) => ({ price, depth: bidSide[1][i] })),
				askSide[0].map((price: bigint, i: number) => ({ price, depth: askSide[1][i] })),
			];
		} else {
			const result = results.results![0].returnValues!.map(([bytes, _]) =>
				bcs
					.vector(bcs.U64)
					.parse(Uint8Array.from(bytes))
					.map((s) => BigInt(s)),
			);
			return result[0].map((price: bigint, i: number) => ({ price, depth: result[1][i] }));
		}
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

	public async getCoinType(coinId: string) {
		const resp = await this.suiClient.getObject({
			id: coinId,
			options: { showType: true },
		});

		const parsed = resp.data?.type != null ? parseStructTag(resp.data.type) : null;

		// Modification handle case like 0x2::coin::Coin<0xf398b9ecb31aed96c345538fb59ca5a1a2c247c5e60087411ead6c637129f1c4::fish::FISH>
		if (
			parsed?.address === NORMALIZED_SUI_COIN_TYPE.split('::')[0] &&
			parsed.module === 'coin' &&
			parsed.name === 'Coin' &&
			parsed.typeParams.length > 0
		) {
			const firstTypeParam = parsed.typeParams[0];
			return typeof firstTypeParam === 'object'
				? firstTypeParam.address + '::' + firstTypeParam.module + '::' + firstTypeParam.name
				: null;
		} else {
			return null;
		}
	}

	#nextClientOrderId() {
		const id = this.clientOrderId;
		this.clientOrderId += 1;
		return id;
	}
}
