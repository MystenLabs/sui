// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Coins } from '_pages/swap/constants';
import { FEATURES } from '_shared/experimentation/features';
import { useFeatureValue } from '@growthbook/growthbook-react';

export const mainnetDeepBook: {
	pools: Record<string, string[]>;
	coinsMap: Record<Coins, string>;
} = {
	pools: {
		SUI_USDC: [
			'0x4405b50d791fd3346754e8171aaab6bc2ed26c2c46efdd033c14b30ae507ac33',
			'0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7',
		],
		WETH_USDC: ['0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826'],
		TBTC_USDC: ['0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899'],
		USDT_USDC: ['0x5deafda22b6b86127ea4299503362638bea0ca33bb212ea3a67b029356b8b955'],
	},
	coinsMap: {
		[Coins.SUI]: '0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
		[Coins.USDC]: '0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN',
		[Coins.USDT]: '0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN',
		[Coins.WETH]: '0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN',
		[Coins.TBTC]: '0xbc3a676894871284b3ccfb2eec66f428612000e2a6e6d23f592ce8833c27c973::coin::COIN',
	},
};

export function useDeepBookConfigs() {
	return useFeatureValue(FEATURES.DEEP_BOOK_CONFIGS, mainnetDeepBook);
}
