// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type EndOfEpochInfo } from '@mysten/sui.js';

export function getEpochStorageFundFlow(endOfEpochInfo: EndOfEpochInfo | null) {
    const fundInflow = endOfEpochInfo
        ? BigInt(endOfEpochInfo.storageFundReinvestment) +
          BigInt(endOfEpochInfo.storageCharge) +
          BigInt(endOfEpochInfo.leftoverStorageFundInflow)
        : null;

    const fundOutflow = endOfEpochInfo
        ? BigInt(endOfEpochInfo.storageRebate)
        : null;

    const netInflow =
        fundInflow !== null && fundOutflow !== null
            ? fundInflow - fundOutflow
            : null;

    return { netInflow, fundInflow, fundOutflow };
}
