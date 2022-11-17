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
        <div className="text-sui-grey-75">
            <Text variant="bodySmall" weight="semibold">
                <time dateTime={new Date(date).toISOString()}>{dateStr}</time>
            </Text>
        </div>
    );
}
