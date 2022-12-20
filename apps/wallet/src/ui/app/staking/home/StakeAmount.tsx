// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import { Text } from '_app/shared/text';
import { useFormatCoin } from '_hooks';

//TODO unify StakeAmount and CoinBalance

type Colors = 'gray-90' | 'success-dark' | 'gray-60';

interface StakeAmountProps {
    balance: bigint;
    type: string;
    diffSymbol?: boolean;
    color: Colors;
    symbolColor: Colors | 'steel';
    size: 'heading4' | 'body';
}

function StakeAmount({
    balance,
    type,
    diffSymbol,
    color,
    symbolColor,
    size,
}: StakeAmountProps) {
    const [formatted, symbol] = useFormatCoin(balance, type);

    const symbolSize = diffSymbol ? 'bodySmall' : size;
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
