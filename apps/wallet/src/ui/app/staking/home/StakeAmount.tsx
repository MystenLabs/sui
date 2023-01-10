// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { memo } from 'react';

import { Text } from '_app/shared/text';
import { useFormatCoin } from '_hooks';

//TODO unify StakeAmount and CoinBalance
interface StakeAmountProps {
    balance: bigint;
    variant: 'heading4' | 'body';
    isEarnedRewards?: boolean;
}

function StakeAmount({ balance, variant, isEarnedRewards }: StakeAmountProps) {
    const [formatted, symbol] = useFormatCoin(balance, SUI_TYPE_ARG);
    // Handle case of 0 balance
    const zeroBalanceColor = !!balance;
    const earnRewardColor =
        isEarnedRewards && (zeroBalanceColor ? 'success-dark' : 'gray-60');
    const colorAmount = variant === 'heading4' ? 'gray-90' : 'steel-darker';
    const colorSymbol = variant === 'heading4' ? 'steel' : 'steel-darker';

    return (
        <div className="flex gap-0.5 align-baseline flex-nowrap items-baseline">
            <Text
                variant={variant}
                weight="semibold"
                color={earnRewardColor || colorAmount}
            >
                {formatted}
            </Text>
            <Text
                variant={variant === 'heading4' ? 'bodySmall' : 'body'}
                color={earnRewardColor || colorSymbol}
                weight="medium"
            >
                {symbol}
            </Text>
        </div>
    );
}

export default memo(StakeAmount);
