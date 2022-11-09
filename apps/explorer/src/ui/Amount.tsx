// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export type AmountProps = { amount: number; symbol?: string };

export function Amount({ amount, symbol }: AmountProps) {
    return (
        <div className="flex flex-row items-end gap-1 text-sui-grey-100 ml-6">
            <Heading as="h4" variant="heading4">
                {amount}
            </Heading>
            {symbol && (
                <div className="text-sui-grey-80">
                    <Text variant="bodySmall">{symbol}</Text>
                </div>
            )}
        </div>
    );
}
