// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient, useTimeAgo } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useEpochProgress(suffix: string = 'left') {
    const rpc = useRpcClient();
    const { data } = useQuery(['system', 'state'], () =>
        rpc.getLatestSuiSystemState()
    );

    const start = +(data?.epochStartTimestampMs ?? 0);
    const duration = +(data?.epochDurationMs ?? 0);
    const end = start + duration;
    const time = useTimeAgo(end, true, true);
    const progress =
        start && duration
            ? Math.min(((Date.now() - start) / (end - start)) * 100, 100)
            : 0;

    return {
        epoch: data?.epoch,
        progress,
        label: end <= Date.now() ? 'Ending soon' : `${time} ${suffix}`,
    };
}
