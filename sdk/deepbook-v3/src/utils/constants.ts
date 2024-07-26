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
	DEEPBOOK_PACKAGE_ID: '0xbf8c499152c205dc5af0a89e95911e2c7d6d1c8b6a4d3a96058aee5fef3ae03a',
	REGISTRY_ID: '0x1e9619bb7e1ffb94f2ba9d1bd55d922921931e138dd0b7a1a3c5ede8d1cf9424',
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
		address: `0xfd25055cdcfff6ef7031c941d4fd407880fa19dd24d0f526c3b38c633fefe49e`,
		baseCoin: 'DEEP',
		quoteCoin: 'SUI',
	},
	SUI_DBUSDC: {
		address: `0x9b7d3cf0c35cdebb9909705548249c4240004f7300e1b0dfc90ac232c8f431d1`,
		baseCoin: 'SUI',
		quoteCoin: 'DBUSDC',
	},
	DEEP_DBUSDC: {
		address: `0xa136f878686acd23ea6881f31b937ffd378bbdb4eaebf54bd793c5f9e2ab53f0`,
		baseCoin: 'DEEP',
		quoteCoin: 'DBUSDC',
	},
	DBUSDT_DBUSDC: {
		address: `0x19a667bb5c80ee40155b8bf9aada0d787d7a86cb44f51b4b8a88df7d5a4cd0d0`,
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
