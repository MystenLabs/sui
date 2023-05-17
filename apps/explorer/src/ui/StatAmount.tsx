// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Amount, type AmountProps } from '~/ui/Amount';
import { DateCard } from '~/ui/DateCard';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export interface StatAmountProps extends Omit<AmountProps, 'size'> {
    dollarAmount?: number;
    date?: Date | number | null;
}

export function StatAmount({ dollarAmount, date, ...props }: StatAmountProps) {
    return (
        <div className="flex flex-col justify-start gap-2 text-gray-75">
            <div className="flex flex-col items-baseline gap-2.5 text-gray-100">
                {date ? <DateCard date={date} /> : null}
                <div className="flex flex-col items-baseline gap-2.5">
                    <Heading
                        as="h4"
                        variant="heading4/semibold"
                        color="gray-90"
                        fixed
                    >
                        Amount
                    </Heading>

                    <Amount size="lg" {...props} />
                </div>
            </div>
            {dollarAmount && (
                <Text variant="bodySmall/semibold" color="steel-dark">
                    {new Intl.NumberFormat(undefined, {
                        style: 'currency',
                        currency: 'USD',
                    }).format(dollarAmount)}
                </Text>
            )}
        </div>
    );
}
