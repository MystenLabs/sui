// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTimeAgo } from '@mysten/core';
import { useMemo } from 'react';

export function useEpochProgress(
    start?: number,
    end?: number,
    suffix: string = 'left'
) {
    const time = useTimeAgo(end);
    const progress = useMemo(
        () => (start && end ? ((Date.now() - start) / (end - start)) * 100 : 0),
        [start, end]
    );
    return {
        progress,
        label: `${time} ${suffix}`,
    };
}
