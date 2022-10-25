// Copyright (c) Mysten Labs, Inc.
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

    return `${MONTHS[date.getUTCMonth()]} ${stdToN(
        date.getUTCDate(),
        2
    )}, ${date.getUTCFullYear()}, ${stdToN(date.getUTCHours(), 2)}:${stdToN(
        date.getUTCMinutes(),
        2
    )}:${stdToN(date.getUTCSeconds(), 2)} UTC`;
};

// TODO - this need a bit of modification to account for multiple display formate types
export const timeAgo = (
    epochMilliSecs: number | null | undefined,
    timeNow?: number,
    shortenTimeLabel?: boolean
): string => {
    if (!epochMilliSecs) return '';

    //In static mode the time is fixed at 1 Jan 2025 01:13:10 UTC for testing purposes
    timeNow = timeNow ? timeNow : IS_STATIC_ENV ? 1735693990000 : Date.now();

    const timeLabel = {
        year: {
            full: 'year',
            short: 'y',
        },
        month: {
            full: 'month',
            short: 'm',
        },
        day: {
            full: 'day',
            short: 'd',
        },
        hour: {
            full: 'hour',
            short: 'h',
        },
        min: {
            full: 'min',
            short: 'm',
        },
        sec: {
            full: 'sec',
            short: 's',
        },
    };
    const dateKeyType = shortenTimeLabel ? 'short' : 'full';

    let timeUnit: [string, number][];
    let timeCol = timeNow - epochMilliSecs;

    if (timeCol >= 1000 * 60 * 60 * 24) {
        timeUnit = [
            [timeLabel.day[dateKeyType], 1000 * 60 * 60 * 24],
            [timeLabel.hour[dateKeyType], 1000 * 60 * 60],
        ];
    } else if (timeCol >= 1000 * 60 * 60) {
        timeUnit = [
            [timeLabel.hour[dateKeyType], 1000 * 60 * 60],
            [timeLabel.min[dateKeyType], 1000 * 60],
        ];
    } else {
        timeUnit = [
            [timeLabel.min[dateKeyType], 1000 * 60],
            [timeLabel.sec[dateKeyType], 1000],
        ];
    }

    const convertAmount = (amount: number, label: string) => {
        const spacing = shortenTimeLabel ? '' : ' ';
        if (amount > 1)
            return `${amount}${spacing}${label}${!shortenTimeLabel ? 's' : ''}`;
        if (amount === 1) return `${amount}${spacing}${label}`;
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
