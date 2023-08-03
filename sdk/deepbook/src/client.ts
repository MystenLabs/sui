// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	DevInspectResults,
	OrderArguments,
	PaginatedEvents,
	PaginationArguments,
} from '@mysten/sui.js/client';
import { SUI_CLOCK_OBJECT_ID } from '@mysten/sui.js/utils';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { MODULE_CLOB, PACKAGE_ID } from './utils';
import { PaginatedPoolSummary } from './types/pool';

const DUMMY_ADDRESS = '0x0';

export class DeepBookClient {
	constructor(
		public suiClient: SuiClient = new SuiClient({ url: getFullnodeUrl('testnet') }),
		public currentAddress: string = DUMMY_ADDRESS,
	) {}

	/**
	 * @description returns paginated list of pools created in DeepBook by querying for the
	 * `PoolCreated` event. Warning: this method can return incomplete results if the upstream data source
	 * is pruned.
	 */
	public async getAllPools(
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
				baseAsset: ('0x' + rawEvent.base_asset.name) as string,
				quoteAsset: ('0x' + rawEvent.quote_asset.name) as string,
			};
		});
		return {
			data: pools,
			nextCursor: resp.nextCursor,
			hasNextPage: resp.hasNextPage,
		};
	}

	/**
	 * @description get the order status
	 * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId: the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param orderId the order id, eg: "1"
	 * @param accountCap: your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3
	 */
	public async getOrderStatus(
		token1: string,
		token2: string,
		poolId: string,
		orderId: string,
		accountCap: string,
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::get_order_status`,
			arguments: [txb.object(poolId), txb.object(orderId), txb.object(accountCap)],
		});
		txb.setSender(this.currentAddress);
		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
	}

	/**
	 * @description: get the base and quote token in custodian account
	 * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param accountCap your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3
	 */
	public async getUsrPosition(
		token1: string,
		token2: string,
		poolId: string,
		accountCap: string,
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::account_balance`,
			arguments: [txb.object(poolId), txb.object(accountCap)],
		});
		txb.setSender(this.currentAddress);
		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
	}

	/**
	 * @description get the open orders of the current user
	 * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param accountCap your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3
	 */
	public async listOpenOrders(
		token1: string,
		token2: string,
		poolId: string,
		accountCap: string,
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
			target: `${PACKAGE_ID}::${MODULE_CLOB}::list_open_orders`,
			arguments: [txb.object(poolId), txb.object(accountCap)],
		});
		txb.setSender(this.currentAddress);

		return await this.suiClient.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: this.currentAddress,
		});
	}

	/**
	 * @description get the market price {bestBidPrice, bestAskPrice}
	 * @param token1 token1 of a certain pair,  eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param token2 token2 of a certain pair,  eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 */
	public async getMarketPrice(token1: string, token2: string, poolId: string) {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
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
	 * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
	 * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
	 * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
	 * @param lowerPrice lower price you want to query in the level2 book, eg: 18000000000
	 * @param higherPrice higher price you want to query in the level2 book, eg: 20000000000
	 * @param isBidSide true: query bid side, false: query ask side
	 */
	public async getLevel2BookStatus(
		token1: string,
		token2: string,
		poolId: string,
		lowerPrice: number,
		higherPrice: number,
		isBidSide: boolean,
	): Promise<DevInspectResults> {
		const txb = new TransactionBlock();
		txb.moveCall({
			typeArguments: [token1, token2],
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
}
