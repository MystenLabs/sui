// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui.js/bcs';
import {
	DevInspectResults,
	OrderArguments,
	PaginatedEvents,
	PaginationArguments,
} from '@mysten/sui.js/client';
import {
	SUI_CLOCK_OBJECT_ID,
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	parseStructTag,
} from '@mysten/sui.js/utils';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { MODULE_CLOB, PACKAGE_ID, NORMALIZED_SUI_COIN_TYPE } from './utils';
import { PaginatedPoolSummary, PoolSummary, UserPosition } from './types/pool';
import { Coin } from '@mysten/sui.js';

const DUMMY_ADDRESS = normalizeSuiAddress('0x0');

export class DeepBookClient {
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
	) {}

	/**
	 * @param cap set the account cap for interacting with DeepBook
	 */
	async setAccountCap(cap: string) {
		this.accountCap = cap;
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
		amount: bigint | number | undefined = undefined,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();

		const { baseAsset, quoteAsset } = await this.getPoolInfo(poolId);
		const hasSui =
			baseAsset === NORMALIZED_SUI_COIN_TYPE || quoteAsset === NORMALIZED_SUI_COIN_TYPE;

		if (coinId === undefined && !hasSui) {
			throw new Error('coinId must be specified if neither baseAsset nor quoteAsset is SUI');
		}

		const inputCoin = coinId ? txb.object(coinId) : txb.gas;

		const [coin] = amount ? txb.splitCoins(inputCoin, [txb.pure(amount)]) : [inputCoin];

		const coinType = coinId ? this.#getCoinType(coinId) : NORMALIZED_SUI_COIN_TYPE;
		if (coinType !== baseAsset && coinType !== quoteAsset) {
			throw new Error(`coin ${coinId} is not a valid asset for pool ${poolId}`);
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
	 * @description construct transaction block for withdrawing asset into a pool.
	 * @param poolId the pool id for the withdraw
	 * @param amount the amount of coin to withdraw
	 * @param assetType Base or Quote
	 * @param recipientAddress the address to receive the withdrawn asset. If omitted, `this.currentAddress` will be used. The function
	 * will throw if the `recipientAddress == DUMMY_ADDRESS`
	 */
	async withdraw(
		poolId: string,
		// TODO: implement withdraw all
		amount: bigint | number,
		assetType: 'Base' | 'Quote',
		recipientAddress: string = this.currentAddress,
	): Promise<TransactionBlock> {
		const txb = new TransactionBlock();

		const { baseAsset, quoteAsset } = await this.getPoolInfo(poolId);

		const functionName = assetType === 'Base' ? 'withdraw_base' : 'withdraw_quote';

		const [withdraw] = txb.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::${functionName}`,
			arguments: [txb.object(poolId), txb.pure(amount), txb.object(this.#checkAccountCap())],
		});
		txb.transferObjects([withdraw], txb.pure(this.#checkCurrentAddress(recipientAddress)));
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

	/**
	 * @description get the order status
	 * @param baseAssetType baseAssetType of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param quoteAssetType quoteAssetType of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId: the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param orderId the order id, eg: "1"
	 * @param accountCap: accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3. If not provided, `this.accountCap` will be used.
	 */
	async getOrderStatus(
		baseAssetType: string,
		quoteAssetType: string,
		poolId: string,
		orderId: string,
		accountCap: string | undefined,
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		const cap = this.#checkAccountCap(accountCap);
		txb.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_order_status`,
			arguments: [txb.object(poolId), txb.object(orderId), txb.object(cap)],
		});
		txb.setSender(this.currentAddress);
		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
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
		const { baseAsset, quoteAsset } = await this.getPoolInfo(poolId);

		txb.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::account_balance`,
			arguments: [txb.object(normalizeSuiObjectId(poolId)), txb.object(cap)],
		});
		txb.setSender(this.currentAddress);
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
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		const cap = this.#checkAccountCap(accountCap);
		const { baseAsset, quoteAsset } = await this.getPoolInfo(poolId);

		txb.moveCall({
			typeArguments: [baseAsset, quoteAsset],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::list_open_orders`,
			arguments: [txb.object(poolId), txb.object(cap)],
		});
		txb.setSender(this.currentAddress);

		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
	}

	/**
	 * @description get the market price {bestBidPrice, bestAskPrice}
	 * @param baseAssetType baseAssetType of a certain pair,  eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param quoteAssetType quoteAssetType of a certain pair,  eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 */
	async getMarketPrice(baseAssetType: string, quoteAssetType: string, poolId: string) {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_market_price`,
			arguments: [txb.object(poolId)],
		});
		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
	}

	/**
	 * @description get level2 book status
	 * @param baseAssetType baseAssetType of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param quoteAssetType quoteAssetType of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param lowerPrice lower price you want to query in the level2 book, eg: 18000000000
	 * @param higherPrice higher price you want to query in the level2 book, eg: 20000000000
	 * @param isBidSide true: query bid side, false: query ask side
	 */
	async getLevel2BookStatus(
		baseAssetType: string,
		quoteAssetType: string,
		poolId: string,
		lowerPrice: number,
		higherPrice: number,
		isBidSide: boolean,
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [baseAssetType, quoteAssetType],
			target: isBidSide
				? `${PACKAGE_ID}::${MODULE_CLOB}::get_level2_book_status_bid_side`
				: `${PACKAGE_ID}::${MODULE_CLOB}::get_level2_book_status_ask_side`,
			arguments: [
				txb.object(poolId),
				txb.pure(String(lowerPrice)),
				txb.pure(String(higherPrice)),
				txb.object(SUI_CLOCK_OBJECT_ID),
			],
		});
		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
	}

	#checkAccountCap(accountCap: string | undefined = undefined): string {
		const cap = accountCap ?? this.accountCap;
		if (cap === undefined) {
			throw new Error('accountCap is undefined, please call setAccountCap() first');
		}
		return normalizeSuiObjectId(cap);
	}

	#checkCurrentAddress(recipientAddress: string): string {
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
}
