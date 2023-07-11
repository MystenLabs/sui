// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type GRAPH_DURATIONS, type UNITS } from './utils';

export type EpochGasInfo = {
	epoch: number;
	referenceGasPrice: bigint | null;
	date: Date | null;
};

export type UnitsType = (typeof UNITS)[number];
export type GraphDurationsType = (typeof GRAPH_DURATIONS)[number];
