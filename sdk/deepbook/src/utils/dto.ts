// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type Pool = {
	clob: string;
	type: string;
	priceDecimals: number;
	amountDecimals: number;
	tickSize: number;
};

export type PoolInfo = {
	needChange: boolean;
	clob: string;
	type: string;
	tickSize: number;
};

export type Records = {
	pools: Pool[];
	tokens: Token[];
	caps: Cap[];
};

export type Token = {
	symbol: string;
	type: string;
	decimals: number;
};

export type Cap = {
	owner: string;
	cap: string;
};
