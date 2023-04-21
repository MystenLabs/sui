// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetSystemState, useTimeAgo } from '@mysten/core';

export function useEpochProgress(suffix: string = 'left') {
    const { data } = useGetSystemState();

    const start = Number(data?.epochStartTimestampMs ?? 0);
    const duration = Number(data?.epochDurationMs ?? 0);
    const end = start + duration;
    const time = useTimeAgo(end, true, true);
    const progress =
        start && duration
            ? Math.min(((Date.now() - start) / (end - start)) * 100, 100)
            : 0;

    const timeLeftMs = Date.now() - end;
    const timeLeftMin = Math.floor(timeLeftMs / 60000);

    let label;
    if (timeLeftMs >= 0) {
        label = 'Ending soon';
    } else if (timeLeftMin >= -1) {
        label = 'About a min left';
    } else {
        label = `${time} ${suffix}`;
    }

    return {
        epoch: data?.epoch,
        progress,
        label,
    };
}

export function getElapsedTime(start: number, end: number) {
    const diff = end - start;

    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    const displayMinutes = minutes - hours * 60;
    const displaySeconds = seconds - minutes * 60;

    const renderTime = [];

    if (hours > 0) {
        renderTime.push(`${hours}h`);
    }
    if (displayMinutes > 0) {
        renderTime.push(`${displayMinutes}m`);
    }
    if (displaySeconds > 0) {
        renderTime.push(`${displaySeconds}s`);
    }

    return renderTime.join(' ');
}
