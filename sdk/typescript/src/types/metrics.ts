// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { number, object } from 'superstruct';

export const NetworkMetrics = object({
  currentTps: number(),
  tps30Days: number(),
  currentCheckpoint: number(),
  currentEpoch: number(),
  totalAddresses: number(),
  totalObjects: number(),
  totalPackages: number(),
});
