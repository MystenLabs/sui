// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSystemState } from './useSystemState';

// Get time between current epoch and specified epoch
// Get the period between the current epoch and next epoch
export function useGetTimeBeforeEpochNumber(epoch: number) {
    const data = useSystemState();
    // Current epoch
    const currentEpoch = +(data.data?.epoch || 0);
    const currentEpochStartTime = +(data.data?.epochStartTimestampMs || 0);
    const epochPeriod = +(data.data?.epochDurationMs || 0);
    const timeBeforeSpecifiedEpoch =
        epoch > currentEpoch && epoch > 0 && epochPeriod > 0
            ? currentEpochStartTime + (epoch - currentEpoch) * epochPeriod
            : 0;

    return {
        ...data,
        data: timeBeforeSpecifiedEpoch,
    };
}
