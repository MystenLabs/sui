// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type EpochGasInfo = {
    epoch: number;
    referenceGasPrice: bigint | null;
    date: Date | null;
};
