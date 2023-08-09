// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { array, number, object, string } from 'superstruct';

export const NetworkMetrics = object({
	currentTps: number(),
	tps30Days: number(),
	currentCheckpoint: string(),
	currentEpoch: string(),
	totalAddresses: string(),
	totalObjects: string(),
	totalPackages: string(),
});
export type NetworkMetrics = typeof NetworkMetrics.TYPE;

export const AddressMetrics = object({
	checkpoint: number(),
	epoch: number(),
	timestampMs: number(),
	cumulativeAddresses: number(),
	cumulativeActiveAddresses: number(),
	dailyActiveAddresses: number(),
});
export type AddressMetrics = typeof AddressMetrics.TYPE;

export const AllEpochsAddressMetrics = array(AddressMetrics);
export type AllEpochsAddressMetrics = typeof AllEpochsAddressMetrics.TYPE;
