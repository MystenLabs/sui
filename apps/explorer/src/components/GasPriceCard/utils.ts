// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, formatBalance } from '@mysten/core';
import { SUI_DECIMALS } from '@mysten/sui.js';

import { type EpochGasInfo, type GraphDurationsType } from './types';

export const UNITS = ['MIST', 'SUI'] as const;
export const GRAPH_DURATIONS = ['7 Epochs', '30 Epochs'] as const;
export const GRAPH_DURATIONS_MAP: Record<GraphDurationsType, number> = {
	'7 Epochs': 7,
	'30 Epochs': 30,
};

export function useGasPriceFormat(gasPrice: bigint | null, unit: 'MIST' | 'SUI') {
	return gasPrice !== null
		? formatBalance(gasPrice, unit === 'MIST' ? 0 : SUI_DECIMALS, CoinFormat.FULL)
		: null;
}

export function isDefined(d: EpochGasInfo) {
	return d.date !== null && d.referenceGasPrice !== null;
}
