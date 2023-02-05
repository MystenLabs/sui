// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { Text } from '_app/shared/text';
import { useFormatCoin } from '_hooks';

//TODO create variant for different use cases like heading4, subtitle, bodySmall and different symbols color
interface CoinBalanceProps {
    amount: bigint | number | string;
    coinType?: string;
}

export function CoinBalance({ amount, coinType }: CoinBalanceProps) {
    const [formatted, symbol] = useFormatCoin(amount, coinType || SUI_TYPE_ARG);

    return Math.abs(Number(amount)) > 0 ? (
        <div className="flex gap-0.5 align-baseline flex-nowrap items-baseline">
            <Text variant="body" weight="semibold" color="gray-90">
                {formatted}
            </Text>
            <Text variant="subtitle" color="gray-90" weight="medium">
                {symbol}
            </Text>
        </div>
    ) : null;
}
