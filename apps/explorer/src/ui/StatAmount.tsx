// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG, Coin } from '@mysten/sui.js';

import { numberSuffix } from '../utils/numberUtil';

import { CoinFormat, formatBalance } from '~/hooks/useFormatCoin';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export interface StatAmountProps {
    amount: bigint | string | number;
    currency?: string;
    dollarAmount?: number;
    date?: string;
    full?: boolean;
}

const SUI_DECIMALS = 9;

export function StatAmount({
    amount,
    currency = SUI_TYPE_ARG,
    dollarAmount,
    date,
    full,
}: StatAmountProps) {
    const formattedAmount = formatBalance(
        amount,
        SUI_DECIMALS,
        full ? CoinFormat.FULL : CoinFormat.ROUNDED
    );
    const coinSymbol =
        currency === SUI_TYPE_ARG ? Coin.getCoinSymbol(SUI_TYPE_ARG) : currency;
    return (
        <div className="flex flex-col justify-start h-full text-sui-grey-75 gap-2">
            <div className="text-sui-grey-100 flex flex-col items-baseline gap-2.5">
                {date && (
                    <div className="text-sui-grey-75">
                        <Text variant="bodySmall" weight="semibold">
                            {date}
                        </Text>
                    </div>
                )}
                <Heading as="h4" variant="heading4" weight="semibold">
                    Amount
                </Heading>

                <div className="flex flex-row items-start gap-1">
                    <Heading as="h2" variant="heading2" weight="bold">
                        {formattedAmount}
                    </Heading>
                    <div className="text-sui-grey-80 ">
                        <Text variant="bodySmall">{coinSymbol}</Text>
                    </div>
                </div>
            </div>
            {dollarAmount && (
                <Text variant="bodySmall" weight="semibold">
                    ${numberSuffix(dollarAmount)}
                </Text>
            )}
        </div>
    );
}
