// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSystemState } from './useSystemState';

// Get start time for the current epoch and specified epoch
// Get the period between the current epoch and next epoch
export function useGetTimeBeforeEpochNumber(epoch: number) {
    // Get current epoch
    const data = useSystemState();
    // Get current epoch
    const currentEpoch = data.data?.epoch || 0;
    const currentEpochStartTime = data.data?.epochStartTimestampMs || 0;
    // TODO: Get the period between epochs from system state
    // setting to 0 until we get the period from system state
    const epochPeriod = 0;
    const timeBeforeSpecifiedEpoch =
        epoch > currentEpoch && epoch > 0 && epochPeriod > 0
            ? currentEpochStartTime + (epoch - currentEpoch) * epochPeriod
            : 0;
    return {
        ...data,
        data: timeBeforeSpecifiedEpoch,
    };
}
