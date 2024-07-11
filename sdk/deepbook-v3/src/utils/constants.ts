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
	DEEPBOOK_PACKAGE_ID: '0xdc1b11f060e96cb30092991d361aff6d78a7c3e9df946df5850a26f9a96b8778',
	REGISTRY_ID: '0x57fea19ce09abf8879327507fa850753f7c6bd468a74971146c38e92aaa39e37',
	DEEP_TREASURY_ID: '0x69fffdae0075f8f71f4fa793549c11079266910e8905169845af1f5d00e09dcb',
} satisfies DeepbookPackageIds;

export const mainnetPackageIds = {
	DEEPBOOK_PACKAGE_ID: '',
	REGISTRY_ID: '',
	DEEP_TREASURY_ID: '',
};

export const testnetCoins: CoinMap = {
	DEEP: {
		key: 'DEEP',
		address: `0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8`,
		type: `0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8::deep::DEEP`,
		scalar: 1000000,
	},
	SUI: {
		key: 'SUI',
		address: `0x0000000000000000000000000000000000000000000000000000000000000002`,
		type: `0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI`,
		scalar: 1000000000,
	},
	DBUSDC: {
		key: 'DBUSDC',
		address: `0xd5aa5b65d97ed7fc0c2b063689805353d56f64f7e8407ac3b95b7e6fdea2256f`,
		type: `0xd5aa5b65d97ed7fc0c2b063689805353d56f64f7e8407ac3b95b7e6fdea2256f::DBUSDC::DBUSDC`,
		scalar: 1000000,
	},
	DBWETH: {
		key: 'DBWETH',
		address: `0xd5aa5b65d97ed7fc0c2b063689805353d56f64f7e8407ac3b95b7e6fdea2256f`,
		type: `0xd5aa5b65d97ed7fc0c2b063689805353d56f64f7e8407ac3b95b7e6fdea2256f::DBWETH::DBWETH`,
		scalar: 100000000,
	},
};

export const mainnetCoins: CoinMap = {
	DEEP: {
		key: 'DEEP',
		address: `0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270`,
		type: `0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270::deep::DEEP`,
		scalar: 1000000,
	},
	SUI: {
		key: 'SUI',
		address: `0x0000000000000000000000000000000000000000000000000000000000000002`,
		type: `0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI`,
		scalar: 1000000000,
	},
	USDC: {
		key: 'USDC',
		address: `0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf`,
		type: `0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN`,
		scalar: 1000000,
	},
	WETH: {
		key: 'WETH',
		address: `0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5`,
		type: `0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN`,
		scalar: 100000000,
	},
};

export const testnetPools: PoolMap = {
	DEEP_SUI: {
		address: `0x67800bae6808206915c7f09203a00031ce9ce8550008862dda3083191e3954ca`,
		baseCoin: testnetCoins.DEEP,
		quoteCoin: testnetCoins.SUI,
	},
	SUI_DBUSDC: {
		address: `0x9442afa775e90112448f26a8d58ca76f66cf46e4b77e74d6d85cea30bedc289c`,
		baseCoin: testnetCoins.SUI,
		quoteCoin: testnetCoins.DBUSDC,
	},
	DEEP_DBWETH: {
		address: `0xe8d0f3525518aaaae64f3832a24606a9eadde8572d058c45626a4ab2cbfae1eb`,
		baseCoin: testnetCoins.DEEP,
		quoteCoin: testnetCoins.DBWETH,
	},
	DBWETH_DBUSDC: {
		address: `0x31d41c00e99672b9f7896950fe24e4993f88fb30a8e05dcd75a24cefe7b7d2d1`,
		baseCoin: testnetCoins.DBWETH,
		quoteCoin: testnetCoins.DBUSDC,
	},
};

export const mainnetPools: PoolMap = {
	DEEP_SUI: {
		address: ``,
		baseCoin: testnetCoins.DEEP,
		quoteCoin: testnetCoins.SUI,
	},
};
