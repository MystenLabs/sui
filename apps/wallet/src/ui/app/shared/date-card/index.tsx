// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { formatDate } from '_helpers';

export interface DateCardProps {
    date: Date | number;
}

// TODO - add format options
export function DateCard({ date }: DateCardProps) {
    const dateStr = formatDate(date, [
        'month',
        'day',
        'year',
        'hour',
        'minute',
    ]);

    return (
        <Text variant="bodySmall" weight="medium" color="steel-dark">
            <time dateTime={new Date(date).toISOString()}>{dateStr}</time>
        </Text>
    );
}
