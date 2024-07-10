// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export interface BalanceManager {
	address: string;
	tradeCap: string | undefined;
}

export interface Coin {
	key: CoinKey;
	address: string;
	type: string;
	scalar: number;
	coinId: string;
}

export interface Pool {
	address: string;
	baseCoin: Coin;
	quoteCoin: Coin;
}

export enum CoinKey {
	'DEEP',
	'SUI',
	'DBUSDC',
	'DBWETH',
	'USDC',
	'WETH',
}

export enum PoolKey {
	'DEEP_SUI',
	'SUI_DBUSDC',
	'DEEP_DBWETH',
	'DBWETH_DBUSDC',
}

// Trading constants
export enum OrderType {
	NO_RESTRICTION,
	IMMEDIATE_OR_CANCEL,
	FILL_OR_KILL,
	POST_ONLY,
}

// Self matching options
export enum SelfMatchingOptions {
	SELF_MATCHING_ALLOWED,
	CANCEL_TAKER,
	CANCEL_MAKER,
}

export interface PlaceLimitOrderParams {
	poolKey: PoolKey;
	managerKey: string;
	clientOrderId: number;
	price: number;
	quantity: number;
	isBid: boolean;
	expiration?: number;
	orderType?: OrderType;
	selfMatchingOption?: SelfMatchingOptions;
	payWithDeep?: boolean;
}

export interface PlaceMarketOrderParams {
	poolKey: PoolKey;
	managerKey: string;
	clientOrderId: number;
	quantity: number;
	isBid: boolean;
	selfMatchingOption?: SelfMatchingOptions;
	payWithDeep?: boolean;
}

export interface ProposalParams {
	poolKey: PoolKey;
	managerKey: string;
	takerFee: number;
	makerFee: number;
	stakeRequired: number;
}

export interface SwapParams {
	poolKey: PoolKey;
	coinKey: CoinKey;
	amount: number;
	deepAmount: number;
}

export interface CreatePoolAdminParams {
	baseCoinKey: CoinKey;
	quoteCoinKey: CoinKey;
	tickSize: number;
	lotSize: number;
	minSize: number;
	whitelisted: boolean;
	stablePool: boolean;
}

export interface Config {
	DEEPBOOK_PACKAGE_ID: string;
	REGISTRY_ID: string;
	DEEP_TREASURY_ID: string;
}

export type Environment = 'mainnet' | 'testnet' | 'devnet' | 'localnet';
