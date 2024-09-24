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
	DEEPBOOK_PACKAGE_ID: '0xc819e689055c241fa1f35a4c87217809edc11b2e73a10fe93300f623e434ecf4',
	REGISTRY_ID: '0xc0bc8c0e0ebd8f891fca1ec0d553c4a06c546a3fa1e902e8fced0606b41a72db',
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
		address: `0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7`,
		type: `0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC`,
		scalar: 6,
	},
	WUSDC: {
		address: `0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf`,
		type: `0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN`,
		scalar: 1000000,
	},
	WETH: {
		address: `0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5`,
		type: `0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN`,
		scalar: 100000000,
	},
	WBTC: {
		address: `0x027792d9fed7f9844eb4839566001bb6f6cb4804f66aa2da6fe1ee242d896881`,
		type: `0x027792d9fed7f9844eb4839566001bb6f6cb4804f66aa2da6fe1ee242d896881::coin::COIN`,
		scalar: 0,
	},
	WUSDT: {
		address: `0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c`,
		type: `0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN`,
		scalar: 1000000,
	},
};

export const testnetPools: PoolMap = {
	DEEP_SUI: {
		address: `0xa603106be87802b9bf737d21c11e2c6e62686d8cac483caa7768c38221158b62`,
		baseCoin: 'DEEP',
		quoteCoin: 'SUI',
	},
	SUI_DBUSDC: {
		address: `0x0b1fb673437c48596806a4a2e925be1e934a0b2f0af93711a47f56b7ac22396a`,
		baseCoin: 'SUI',
		quoteCoin: 'DBUSDC',
	},
	DEEP_DBUSDC: {
		address: `0x27b3a6620b5a417fb1d1e7cbd0085e7954f786d88838d5c72eaa0469c86643f0`,
		baseCoin: 'DEEP',
		quoteCoin: 'DBUSDC',
	},
	DBUSDT_DBUSDC: {
		address: `0x997d59f07ea33df3d550687fe3aa5043c7e3b61e00c53a865dda7612fd8ab3f2`,
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
	SUI_USDC: {
		address: ``,
		baseCoin: 'SUI',
		quoteCoin: 'USDC',
	},
	DEEP_USDC: {
		address: ``,
		baseCoin: 'DEEP',
		quoteCoin: 'USDC',
	},
	USDT_USDC: {
		address: ``,
		baseCoin: 'USDT',
		quoteCoin: 'USDC',
	},
	WUSDC_USDC: {
		address: ``,
		baseCoin: 'WUSDC',
		quoteCoin: 'USDC',
	},
};
