// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { useTimeAgo } from '~/utils/timeUtils';

export function useEpochProgress(
    start?: number,
    end?: number,
    suffix: string = 'left'
) {
    const [number, label] = useTimeAgo(end).split(' ');
    const progress = useMemo(
        () => (start && end ? ((Date.now() - start) / (end - start)) * 100 : 0),
        [start, end]
    );
    return {
        progress,
        label: `${number} ${label} ${suffix}`,
    };
}
