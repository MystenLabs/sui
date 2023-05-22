// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { number, object, string } from 'superstruct';

export const NetworkMetrics = object({
  currentTps: number(),
  tps30Days: number(),
  currentCheckpoint: string(),
  currentEpoch: string(),
  totalAddresses: string(),
  totalObjects: string(),
  totalPackages: string(),
});

export const AddressMetrics = object({
  checkpoint: string(),
  epoch: string(),
  timestampMs: string(),
  cumulativeAddresses: string(),
  cumulativeActiveAddresses: string(),
  dailyActiveAddresses: string(),
})