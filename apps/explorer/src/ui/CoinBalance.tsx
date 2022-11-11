// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

// Passing amount as a string or number for optional number suffix
export type CoinBalanceProps = { amount: number | string; symbol?: string };

export function CoinBalance({ amount, symbol }: CoinBalanceProps) {
    return (
        <div className="flex items-end gap-1 text-sui-grey-100 break-words">
            <Heading variant="heading4">{amount}</Heading>
            {symbol && (
                <div className="text-sui-grey-80">
                    <Text variant="bodySmall">{symbol}</Text>
                </div>
            )}
        </div>
    );
}
