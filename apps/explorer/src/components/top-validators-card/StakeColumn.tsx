// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, CoinFormat } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { Text } from '~/ui/Text';

type StakeColumnProps = {
    stake: bigint | number | string;
    hideCoinSymbol?: boolean;
};

export function StakeColumn({ stake, hideCoinSymbol }: StakeColumnProps) {
    const [amount, symbol] = useFormatCoin(
        stake,
        SUI_TYPE_ARG,
        hideCoinSymbol ? CoinFormat.FULL : CoinFormat.ROUNDED
    );
    return (
        <div className="flex items-end gap-0.5">
            <Text variant="bodySmall/medium" color="steel-darker">
                {amount}
            </Text>
            {!hideCoinSymbol && (
                <Text variant="captionSmall/medium" color="steel-dark">
                    {symbol}
                </Text>
            )}
        </div>
    );
}
