// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react';

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

const TIME_LABEL = {
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

const ONE_SECOND = 1000;
const ONE_MINUTE = ONE_SECOND * 60;
const ONE_HOUR = ONE_MINUTE * 60;
const ONE_DAY = ONE_HOUR * 24;

/**
 * Formats a timestamp using `timeAgo`, and automatically updates it when the value is small.
 */
export function useTimeAgo(
    timeFrom?: number | null,
    shortedTimeLabel?: boolean
) {
    const [now, setNow] = useState(() => Date.now());
    const formattedTime = useMemo(
        () => timeAgo(timeFrom, now, shortedTimeLabel),
        [now, timeFrom, shortedTimeLabel]
    );

    const intervalEnabled = !!timeFrom && now - timeFrom < ONE_HOUR;

    useEffect(() => {
        if (!timeFrom || !intervalEnabled) return;

        const timeout = setInterval(() => setNow(Date.now()), ONE_SECOND);
        return () => clearTimeout(timeout);
    }, [intervalEnabled, timeFrom]);

    return formattedTime;
}

// TODO - this need a bit of modification to account for multiple display formate types
export const timeAgo = (
    epochMilliSecs: number | null | undefined,
    timeNow?: number | null,
    shortenTimeLabel?: boolean
): string => {
    if (!epochMilliSecs) return '';

    //In static mode the time is fixed at 1 Jan 2025 01:13:10 UTC for testing purposes
    timeNow = timeNow ? timeNow : IS_STATIC_ENV ? 1735693990000 : Date.now();

    const dateKeyType = shortenTimeLabel ? 'short' : 'full';

    let timeUnit: [string, number][];
    let timeCol = timeNow - epochMilliSecs;

    if (timeCol >= ONE_DAY) {
        timeUnit = [
            [TIME_LABEL.day[dateKeyType], ONE_DAY],
            [TIME_LABEL.hour[dateKeyType], ONE_HOUR],
        ];
    } else if (timeCol >= ONE_HOUR) {
        timeUnit = [
            [TIME_LABEL.hour[dateKeyType], ONE_HOUR],
            [TIME_LABEL.min[dateKeyType], ONE_MINUTE],
        ];
    } else {
        timeUnit = [
            [TIME_LABEL.min[dateKeyType], ONE_MINUTE],
            [TIME_LABEL.sec[dateKeyType], ONE_SECOND],
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

// TODO - Merge with related functions
type Format =
    | 'year'
    | 'month'
    | 'day'
    | 'hour'
    | 'minute'
    | 'second'
    | 'weekday';

export function formatDate(date: Date | number, format?: Format[]): string {
    const formatOption =
        format ?? (['month', 'day', 'hour', 'minute'] as Format[]);
    const dateTime = new Date(date);
    if (!(dateTime instanceof Date)) return '';

    const options = {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        weekday: 'short',
        minute: 'numeric',
        second: 'numeric',
    };

    const formatOptions = formatOption.reduce(
        (accumulator, current: Format) => {
            const responseObj = {
                ...accumulator,
                ...{ [current]: options[current] },
            };
            return responseObj;
        },
        {}
    );

    return new Intl.DateTimeFormat('en-US', formatOptions).format(dateTime);
}
