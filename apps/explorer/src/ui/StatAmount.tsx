// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';
import { formatDate } from '~/utils/timeUtils';

export interface StatAmountProps {
    amount: number | string;
    currency?: string;
    dollarAmount?: number | string;
    date?: Date | number;
}

export function StatAmount({
    amount,
    currency = 'SUI',
    dollarAmount,
    date,
}: StatAmountProps) {
    return (
        <div className="flex flex-col justify-start h-full text-sui-grey-75 gap-2">
            <div className="text-sui-grey-100 flex flex-col items-baseline gap-2.5">
                {date && (
                    <div className="text-sui-grey-75">
                        <Text variant="bodySmall" weight="semibold">
                            {formatDate(date, [
                                'month',
                                'day',
                                'year',
                                'hour',
                                'minute',
                            ])}
                        </Text>
                    </div>
                )}
                <Heading as="h4" variant="heading4" weight="semibold">
                    Amount
                </Heading>

                <div className="flex flex-row items-start gap-1">
                    <Heading as="h2" variant="heading2" weight="bold">
                        {amount}
                    </Heading>
                    <div className="text-sui-grey-80 ">
                        <Text variant="bodySmall">{currency}</Text>
                    </div>
                </div>
            </div>
            {dollarAmount && (
                <Text variant="bodySmall" weight="semibold">
                    ${dollarAmount}
                </Text>
            )}
        </div>
    );
}
