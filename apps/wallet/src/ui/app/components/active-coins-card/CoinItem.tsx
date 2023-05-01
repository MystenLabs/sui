// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';

import { Text } from '_app/shared/text';
import { CoinIcon } from '_components/coin-icon';

type CoinItemProps = {
    coinType: string;
    balance: bigint;
    isActive?: boolean;
    usd?: number;
};

export function CoinItem({ coinType, balance, isActive, usd }: CoinItemProps) {
    const [formatted, symbol] = useFormatCoin(balance, coinType);

    return (
        <div className="flex gap-2.5 w-full justify-center items-center">
            <CoinIcon coinType={coinType} size={isActive ? 'sm' : 'md'} />
            <div className="flex flex-1 gap-1.5 justify-between">
                <div className="flex flex-col gap-1.5">
                    <Text variant="body" color="steel-darker" weight="semibold">
                        {symbol} {isActive ? 'available' : ''}
                    </Text>
                    {!isActive ? (
                        <Text
                            variant="subtitle"
                            color="steel-dark"
                            weight="semibold"
                        >
                            {symbol}
                        </Text>
                    ) : null}
                </div>
                <div className="flex flex-row justify-center items-center">
                    {isActive ? (
                        <Text
                            variant="body"
                            color="steel-darker"
                            weight="medium"
                        >
                            {formatted}
                        </Text>
                    ) : (
                        <div className="flex flex-col justify-end items-end gap-1.5">
                            <Text
                                variant="body"
                                color="gray-90"
                                weight="medium"
                            >
                                {formatted} {symbol}
                            </Text>
                            {usd && (
                                <Text
                                    variant="caption"
                                    color="steel-dark"
                                    weight="medium"
                                >
                                    ${usd.toLocaleString('en-US')}
                                </Text>
                            )}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
