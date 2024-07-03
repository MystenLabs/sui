// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useDeepBookConfigs } from '_app/hooks/deepbook/useDeepBookConfigs';
import { useDeepBookContext } from '_shared/deepBook/context';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';

export function useRecognizedCoins() {
	const coinsMap = useDeepBookContext().configs.coinsMap;
	return Object.values(coinsMap);
}

export function useAllowedSwapCoinsList() {
	const deepBookConfigs = useDeepBookConfigs();
	const coinsMap = deepBookConfigs.coinsMap;

	return [SUI_TYPE_ARG, coinsMap.SUI, coinsMap.USDC];
}
