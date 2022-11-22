// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '~/ui/Text';
import { formatDate } from '~/utils/timeUtils';

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

    if (!dateStr) {
        return null;
    }

    return (
        <Text variant="bodySmall" weight="semibold" color="steel-dark">
            <time dateTime={new Date(date).toISOString()}>{dateStr}</time>
        </Text>
    );
}
