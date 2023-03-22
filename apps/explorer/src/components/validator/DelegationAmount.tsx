// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

type DelegationAmountProps = {
    amount: bigint | number | string;
    isStats?: boolean;
};

export function DelegationAmount({ amount, isStats }: DelegationAmountProps) {
    const [formattedAmount, symbol] = useFormatCoin(amount, SUI_TYPE_ARG);

    return isStats ? (
        <div className="flex items-end gap-1.5 break-all">
            <Heading as="div" variant="heading3/semibold" color="steel-darker">
                {formattedAmount}
            </Heading>
            <Heading variant="heading4/medium" color="steel-darker">
                {symbol}
            </Heading>
        </div>
    ) : (
        <div className="flex h-full items-center gap-1">
            <div className="flex items-baseline gap-0.5 break-all text-steel-darker">
                <Text variant="body/medium">{formattedAmount}</Text>
                <Text variant="subtitleSmall/medium">{symbol}</Text>
            </div>
        </div>
    );
}
