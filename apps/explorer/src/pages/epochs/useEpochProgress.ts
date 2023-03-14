// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTimeAgo } from '@mysten/core';

import { useGetSystemObject } from '~/hooks/useGetObject';

export function useEpochProgress(suffix: string = 'left') {
    const { data } = useGetSystemObject();
    const start = data?.epochStartTimestampMs ?? 0;
    const duration = data?.epochDurationMs ?? 0;
    const end = start + duration;

    const time = useTimeAgo(end);
    const progress =
        start && duration
            ? Math.min(((Date.now() - start) / (end - start)) * 100, 100)
            : 0;

    return {
        epoch: data?.epoch,
        progress,
        label: `${time} ${suffix}`,
    };
}
