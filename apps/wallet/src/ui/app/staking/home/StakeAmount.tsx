// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { cva, type VariantProps } from 'class-variance-authority';
import { memo } from 'react';

import { Text } from '_app/shared/text';
import { useFormatCoin } from '_hooks';

//TODO unify StakeAmount and CoinBalance

const textStyles = cva([], {
    variants: {
        variant: {
            heading: 'text-heading4 ',
            body: 'text-body',
        },
    },
});

type Colors = 'gray-90' | 'success-dark' | 'gray-60' | 'steel-darker';

interface StakeAmountProps {
    balance: bigint;
    variant: 'heading' | 'body';
    color: Colors;
    symbolColor: Colors | 'steel';
    size: 'heading4' | 'body';
}

function StakeAmount({
    balance,
    variant,
    color,
    symbolColor,
    size,
}: StakeAmountProps) {
    const [formatted, symbol] = useFormatCoin(balance, SUI_TYPE_ARG);
    const symbolSize = variant === 'heading' ? 'bodySmall' : 'body';

    // Handle case of 0 balance
    const isZeroBalance = !balance;

    //  const color = isZeroBalance ? 'gray-60' : color;

    return (
        <div className="flex gap-0.5 align-baseline flex-nowrap items-baseline">
            <Text variant={size} weight="semibold" color={color}>
                {formatted}
            </Text>
            <Text variant={symbolSize} color={symbolColor} weight="medium">
                {symbol}
            </Text>
        </div>
    );
}

export default memo(StakeAmount);
