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

export const timeAgo = (
    epochMilliSecs: number | null | undefined,
    timeNow?: number
): string => {
    if (!epochMilliSecs) return '';

    //In static mode the time is fixed at 1 Jan 2025 01:13:10 UTC for testing purposes
    timeNow = timeNow ? timeNow : IS_STATIC_ENV ? 1735693990000 : Date.now();

    let timeUnit: [string, number][];
    let timeCol = timeNow - epochMilliSecs;

    if (timeCol >= 1000 * 60 * 60 * 24) {
        timeUnit = [
            ['day', 1000 * 60 * 60 * 24],
            ['hour', 1000 * 60 * 60],
        ];
    } else if (timeCol >= 1000 * 60 * 60) {
        timeUnit = [
            ['hour', 1000 * 60 * 60],
            ['min', 1000 * 60],
        ];
    } else {
        timeUnit = [
            ['min', 1000 * 60],
            ['sec', 1000],
        ];
    }

    const convertAmount = (amount: number, label: string) => {
        if (amount > 1) return `${amount} ${label}s`;
        if (amount === 1) return `${amount} ${label}`;
        return '';
    };

    const resultArr = timeUnit.map(([label, denom]) => {
        const whole = Math.floor(timeCol / denom);
        timeCol = timeCol - whole * denom;
        return convertAmount(whole, label);
    });

    const result = resultArr.join(' ').trim();

    return result ? result : `< 1 sec`;
};
