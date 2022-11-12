// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinBalance } from '~/ui/CoinBalance';
import { DateCard } from '~/ui/DateCard';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export interface StatAmountProps {
    amount: number | string | bigint;
    currency?: string | null;
    dollarAmount?: number;
    date?: Date | number | null;
}

export function StatAmount({
    amount,
    currency,
    dollarAmount,
    date,
}: StatAmountProps) {
    return (
        <div className="flex flex-col justify-start text-sui-grey-75 gap-2">
            <div className="text-sui-grey-100 flex flex-col items-baseline gap-2.5">
                {date && <DateCard date={date} />}
                <div className="flex flex-col items-baseline gap-2.5">
                    <Heading as="h4" variant="heading4" weight="semibold">
                        Amount
                    </Heading>

                    <CoinBalance amount={amount} symbol={currency} size="lg" />
                </div>
            </div>
            {dollarAmount && (
                <Text variant="bodySmall" weight="semibold">
                    {new Intl.NumberFormat(undefined, {
                        style: 'currency',
                        currency: 'USD',
                    }).format(dollarAmount)}
                </Text>
            )}
        </div>
    );
}
