// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type PaginatedCoins = {
	data: CoinStruct[];
	nextCursor: string | null;
	hasNextPage: boolean;
};

export type CoinStruct = {
	coinType: string;
	// TODO(chris): rename this to objectId
	coinObjectId: string;
	version: string;
	digest: string;
	balance: string;
	previousTransaction: string;
};

export type DelegatedStake = {
	validatorAddress: string;
	stakingPool: string;
	stakes: StakeObject[];
};

export type StakeObject = {
	stakedSuiId: string;
	stakeRequestEpoch: string;
	stakeActiveEpoch: string;
	principal: string;
	status: 'Active' | 'Pending' | 'Unstaked';
	estimatedReward: string;
};

export type CoinBalance = {
	coinType: string;
	coinObjectCount: number;
	totalBalance: string;
	lockedBalance: {
		string?: number;
		number?: number;
	};
};

export type CoinSupply = {
	value: string;
};

export type CoinMetadata = {
	decimals: number;
	name: string;
	symbol: string;
	description: string;
	iconUrl: string | null;
	id: string | null;
};
