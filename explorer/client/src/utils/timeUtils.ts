// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { IS_STATIC_ENV } from './envUtil';

const stdToN = (original: number, length: number) =>
    String(original).padStart(length, '0');

export const convertNumberToDate = (epochMilliSecs: number | null): string => {
    if (!epochMilliSecs) return 'Not Available';

    const date = new Date(epochMilliSecs);

    const MONTHS = [
        'Jan',
        'Feb',
        'Mar',
        'Apr',
        'May',
        'Jun',
        'Jul',
        'Aug',
        'Sep',
        'Oct',
        'Nov',
        'Dec',
    ];

    return `${stdToN(date.getUTCDate(), 2)} ${
        MONTHS[date.getUTCMonth()]
    } ${date.getUTCFullYear()} ${stdToN(date.getUTCHours(), 2)}:${stdToN(
        date.getUTCMinutes(),
        2
    )}:${stdToN(date.getUTCSeconds(), 2)} UTC`;
};

export const timeAgo = (epochMilliSecs: number | null): string => {
    if (!epochMilliSecs) return 'Not Available';

    //In static mode the time is fixed at 1 Jan 2025 01:13:10 UTC for testing purposes
    const timeNow = IS_STATIC_ENV ? 1735693990000 : Date.now();

    const timeDiff = timeNow - epochMilliSecs;

    const days = Math.floor(timeDiff / (1000 * 60 * 60 * 24));

    if (days >= 1) return `${days} day${days === 1 ? '' : 's'}`;

    const hours = Math.floor(timeDiff / (1000 * 60 * 60));

    if (hours >= 1) return `${hours} hour${hours === 1 ? '' : 's'}`;

    const mins = Math.floor(timeDiff / (1000 * 60));

    if (mins >= 1) return `${mins} min${mins === 1 ? '' : 's'}`;

    const secs = Math.floor(timeDiff / 1000);

    if (secs >= 1) return `${secs} sec${secs === 1 ? '' : 's'}`;

    return `< 1 sec`;
};
