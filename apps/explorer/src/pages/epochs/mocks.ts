// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type Epoch = {
    epoch: number;
    // validators: any;
    // transactionCount: number;
    checkpointSet: [number, number];
    startTimestamp: number;
    endTimestamp: number;
    totalRewards: number;
    stakeSubsidies: number;
    storageFundEarnings: number;
    storageSize: number;
    transactionCount: number;
    gasCostSummary?: {
        gasRevenue: number;
        totalRevenue: number;
        storageRevenue: number;
        stakeRewards: number;
    };
};

export const recentTime = (future = false) => {
    const now = new Date().getTime();
    const offset = Math.floor(Math.random() * 1000 * 60 * 60 * 24);
    return now + offset * (future ? 1 : -1);
};

export const getMockEpochData = (): Epoch => ({
    epoch: 0,
    storageSize: 1000,
    startTimestamp: recentTime(),
    endTimestamp: recentTime(true),
    stakeSubsidies: 1000000,
    transactionCount: 1000,
    checkpointSet: [12345, 15678],
    gasCostSummary: {
        gasRevenue: 1000000,
        storageRevenue: 1000000,
        stakeRewards: 1000000,
        totalRevenue: 1000000,
    },
    storageFundEarnings: 1000000,
    totalRewards: 1000000,
});

export const getEpochs = async () =>
    Promise.all(Array.from({ length: 20 }).map(getMockEpochData));

// getCheckpoints()
export const getCheckpoints = async () =>
    Promise.all(Array.from({ length: 20 }).map(getCheckpoint));

// getCheckpoint()
export const getCheckpoint = async () => ({
    epoch: 0,
    timestampMs: recentTime(),
    sequence_number: 50000,
    network_total_transactions: 10000,
    content_digest: 'J2ERuhiokCicsQVfgs7bZRqkGmZtSoDtL7yNRH6AXtBd',
    signature: 'J2ERuhiokCicsQVfgs7bZRqkGmZtSoDtL7yNRH6AXtBd',
    previous_digest: 'J2ERuhiokCicsQVfgs7bZRqkGmZtSoDtL7yNRH6AXtBd',
    epoch_rolling_gas_cost_summary: {
        computation_cost: 10000,
        storage_cost: 100000,
        storage_rebate: 100000,
    },
    transaction_count: 1000000,
    transactions: [],
});
