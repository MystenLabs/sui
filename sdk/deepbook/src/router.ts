// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js/transactions';
import { SUI_CLOCK_OBJECT_ID } from '@mysten/sui.js/utils';
import { PoolInfo, Records } from './utils';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { getPoolInfoByRecords, MODULE_CLOB, PACKAGE_ID } from './utils';

export type smartRouteResult = {
	maxSwapTokens: number;
	smartRoute: string[];
};

export type smartRouteResultWithExactPath = {
	txb: TransactionBlock;
	amount: number;
};

export class Router {
	public provider: SuiClient;
	public records: Records;

	constructor(
		provider: SuiClient = new SuiClient({ url: getFullnodeUrl('localnet') }),
		records: Records,
	) {
		this.provider = provider;
		this.records = records;
	}

	/**
	 * @param tokenInObject the tokenObject you want to swap
	 * @param tokenOut the token you want to swap to
	 * @param clientOrderId an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
	 * @param amountIn the amount of token you want to swap
	 * @param isBid true for bid, false for ask
	 * @param currentAddress current user address
	 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount
	 */
	public async findBestRoute(
		tokenInObject: string,
		tokenOut: string,
		clientOrderId: string,
		amountIn: number,
		isBid: boolean,
		currentAddress: string,
		accountCap: string,
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
				clientOrderId,
				isBid,
				amountIn,
				currentAddress,
				accountCap,
				path,
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
	 * @param clientOrderId the client order id
	 * @param isBid true for bid, false for ask
	 * @param amountIn the amount of token you want to swap: eg, 1000000
	 * @param currentAddress your own address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
	 * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
	 * @param path the path you want to swap through, for example, you have found that the best route is wbtc --> usdt --> weth, then the path should be ["0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::wbtc::WBTC", "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT", "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH"]
	 */
	public async placeMarketOrderWithSmartRouting(
		tokenInObject: string,
		tokenTypeOut: string,
		clientOrderId: string,
		isBid: boolean,
		amountIn: number,
		currentAddress: string,
		accountCap: string,
		path: string[],
	): Promise<smartRouteResultWithExactPath | undefined> {
		const txb = new TransactionBlock();
		const tokenIn = txb.object(tokenInObject);
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
					_tokenOut = txb.moveCall({
						typeArguments: [nextPath],
						target: `0x2::coin::zero`,
						arguments: [],
					});
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
					_tokenOut = txb.moveCall({
						typeArguments: [nextPath],
						target: `0x2::coin::zero`,
						arguments: [],
					});
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
					target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_quote_for_base`,
					arguments: [
						txb.object(poolInfo.clob_v2),
						txb.pure(clientOrderId),
						txb.object(accountCap),
						_amount,
						txb.object(SUI_CLOCK_OBJECT_ID),
						_tokenOut,
					],
				});
			} else {
				// here swap_exact_base_for_quote
				[base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
					typeArguments: [isBid ? nextPath : path[i], isBid ? path[i] : nextPath],
					target: `${PACKAGE_ID}::${MODULE_CLOB}::swap_exact_base_for_quote`,
					arguments: [
						txb.object(poolInfo.clob_v2),
						txb.pure(clientOrderId),
						txb.object(accountCap),
						_amount,
						// @ts-ignore
						_tokenIn,
						_tokenOut,
						txb.object(SUI_CLOCK_OBJECT_ID),
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
		return undefined;
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
		res: string[][] = [[]],
	): string[][] {
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
						if (token.indexOf(MODULE_CLOB) !== -1) {
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

		for (const child of children.values()) {
			const result = this.dfs(child, tokenTypeOut, records, [...path, tokenTypeIn], depth, res);
			if (result) {
				return result;
			}
		}
		return res;
	}
}
