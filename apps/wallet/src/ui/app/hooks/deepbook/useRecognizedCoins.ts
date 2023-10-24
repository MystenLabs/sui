// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';

import { Coins, mainnetDeepBook, useDeepBookConfigs } from '.';

export function useRecognizedCoins() {
	const coinsMap = useDeepBookConfigs().coinsMap;
	return Object.values(coinsMap);
}

export const allowedSwapCoinsList = [SUI_TYPE_ARG, mainnetDeepBook.coinsMap[Coins.USDC]];
