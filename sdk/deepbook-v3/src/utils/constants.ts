// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Coin, Pool } from '../types/index.js';

export type CoinMap = Record<string, Coin>;
export type PoolMap = Record<string, Pool>;
export interface DeepbookPackageIds {
	DEEPBOOK_PACKAGE_ID: string;
	REGISTRY_ID: string;
	DEEP_TREASURY_ID: string;
}

export const testnetPackageIds = {
	DEEPBOOK_PACKAGE_ID: '0x279443bdf339c7beb585fa44bbd9fa40e4ff255b4df6a2eca32ccc469ffc9046',
	REGISTRY_ID: '0xe9c839fd0c6469b750b96ebe0fe656eec25466dea2a57f7d6df12c63c71af9ac',
	DEEP_TREASURY_ID: '0x69fffdae0075f8f71f4fa793549c11079266910e8905169845af1f5d00e09dcb',
} satisfies DeepbookPackageIds;

export const mainnetPackageIds = {
	DEEPBOOK_PACKAGE_ID: '',
	REGISTRY_ID: '',
	DEEP_TREASURY_ID: '',
};

export const testnetCoins: CoinMap = {
	DEEP: {
		address: `0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8`,
		type: `0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8::deep::DEEP`,
		scalar: 1000000,
	},
	SUI: {
		address: `0x0000000000000000000000000000000000000000000000000000000000000002`,
		type: `0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI`,
		scalar: 1000000000,
	},
	DBUSDC: {
		address: `0xf7152c05930480cd740d7311b5b8b45c6f488e3a53a11c3f74a6fac36a52e0d7`,
		type: `0xf7152c05930480cd740d7311b5b8b45c6f488e3a53a11c3f74a6fac36a52e0d7::DBUSDC::DBUSDC`,
		scalar: 1000000,
	},
	DBUSDT: {
		address: `0xf7152c05930480cd740d7311b5b8b45c6f488e3a53a11c3f74a6fac36a52e0d7`,
		type: `0xf7152c05930480cd740d7311b5b8b45c6f488e3a53a11c3f74a6fac36a52e0d7::DBUSDT::DBUSDT`,
		scalar: 1000000,
	},
};

export const mainnetCoins: CoinMap = {
	DEEP: {
		address: `0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270`,
		type: `0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270::deep::DEEP`,
		scalar: 1000000,
	},
	SUI: {
		address: `0x0000000000000000000000000000000000000000000000000000000000000002`,
		type: `0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI`,
		scalar: 1000000000,
	},
	USDC: {
		address: `0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf`,
		type: `0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN`,
		scalar: 1000000,
	},
	WETH: {
		address: `0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5`,
		type: `0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN`,
		scalar: 100000000,
	},
	USDT: {
		address: `0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c`,
		type: `0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN`,
		scalar: 1000000,
	},
};

export const testnetPools: PoolMap = {
	DEEP_SUI: {
		address: `0x69c220e16a38e6f18073b2ae5d200d45f27449db86d3435a592474686037416e`,
		baseCoin: 'DEEP',
		quoteCoin: 'SUI',
	},
	SUI_DBUSDC: {
		address: `0x7abf219eda0771e27a4cc1c87946515c3dfc6ef994b4b06aef81830ff3bfad7f`,
		baseCoin: 'SUI',
		quoteCoin: 'DBUSDC',
	},
	DEEP_DBUSDC: {
		address: `0x4d48440f70032595f31d1fd39c4891e81e9b3f5d48ae2dde3198f26eebfed55e`,
		baseCoin: 'DEEP',
		quoteCoin: 'DBUSDC',
	},
	DBUSDT_DBUSDC: {
		address: `0x2d532573987c46e43b6d4c09d6b702caee045d330b7450300d331e57a11a0308`,
		baseCoin: 'DBUSDT',
		quoteCoin: 'DBUSDC',
	},
};

export const mainnetPools: PoolMap = {
	DEEP_SUI: {
		address: ``,
		baseCoin: 'DEEP',
		quoteCoin: 'SUI',
	},
};
