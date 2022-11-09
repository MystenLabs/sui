// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export type AmountProps = { amount: number; coinSymbol?: string };

export function Amount({ amount, coinSymbol }: AmountProps) {
    return (
        <div className="flex flex-row items-end gap-1 text-sui-grey-100 ml-6">
            <Heading as="h4" variant="heading4">
                {amount}
            </Heading>
            {coinSymbol && (
                <div className="text-sui-grey-80">
                    <Text variant="bodySmall">{coinSymbol}</Text>
                </div>
            )}
        </div>
    );
}
