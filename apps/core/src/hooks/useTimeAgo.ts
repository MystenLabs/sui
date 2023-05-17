// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react';

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
    shortedTimeLabel?: boolean,
    shouldEnd?: boolean,
    endLabel?: string
) {
    const [now, setNow] = useState(() => Date.now());

    // end interval when the difference between now and timeFrom is less than or equal to 0
    const continueInterval = shouldEnd ? (timeFrom || now) - now >= 0 : true;
    const intervalEnabled =
        !!timeFrom &&
        Math.abs(now - (timeFrom || now)) < ONE_HOUR &&
        continueInterval;

    const formattedTime = useMemo(
        () => timeAgo(timeFrom, now, shortedTimeLabel, endLabel),
        [now, timeFrom, shortedTimeLabel, endLabel]
    );

    useEffect(() => {
        if (!timeFrom || !intervalEnabled) return;
        const timeout = setInterval(() => setNow(Date.now()), ONE_SECOND);
        return () => clearTimeout(timeout);
    }, [intervalEnabled, timeFrom]);

    return formattedTime;
}

// TODO - this need a bit of modification to account for multiple display format types
export const timeAgo = (
    epochMilliSecs: number | null | undefined,
    timeNow?: number | null,
    shortenTimeLabel?: boolean,
    endLabel = `< 1 sec`
): string => {
    if (!epochMilliSecs) return '';

    timeNow = timeNow ? timeNow : Date.now();
    const dateKeyType = shortenTimeLabel ? 'short' : 'full';

    let timeUnit: [string, number][];
    let timeCol = Math.abs(timeNow - epochMilliSecs);

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

    return result ? result : endLabel;
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
