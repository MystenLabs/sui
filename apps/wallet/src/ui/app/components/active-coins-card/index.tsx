// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { CoinItem } from './CoinItem';
import { Text } from '_app/shared/text';
import { CoinIcon } from '_components/coin-icon';
import { useAppSelector, useFormatCoin } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';

function SelectedCoinCard({
    balance,
    coinType,
}: {
    balance: bigint | number;
    coinType: string;
}) {
    const [formatted, symbol] = useFormatCoin(balance, coinType);

    return (
        <Link
            to={`/send/select?${new URLSearchParams({
                type: coinType,
            }).toString()}`}
            className="bg-gray-40 rounded-2lg py-2.5 px-3 no-underline flex gap-2 items-center w-full"
        >
            <CoinIcon coinType={coinType} size="sm" />

            <div className="flex flex-1 w-full justify-between">
                <Text variant="body" color="steel-darker" weight="semibold">
                    {symbol} available
                </Text>
                <Text variant="body" color="steel-darker" weight="medium">
                    {formatted}
                </Text>
            </div>
        </Link>
    );
}

export function ActiveCoinsCard({
    activeCoinType = SUI_TYPE_ARG,
    showActiveCoin = true,
}: {
    activeCoinType: string;
    showActiveCoin?: boolean;
}) {
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);

    const coins = useMemo(
        () =>
            Object.entries(aggregateBalances).map((aType) => {
                return {
                    coinType: aType[0],
                    balance: aType[1],
                };
            }),
        [aggregateBalances]
    );

    const activeCoin = useMemo(() => {
        return coins.filter((coin) => coin.coinType === activeCoinType)[0];
    }, [activeCoinType, coins]);

    const CoinListCard = (
        <div className="flex flex-col w-full">
            <Text variant="caption" color="steel-darker" weight="semibold">
                My Coins
            </Text>
            <div className="flex flex-col justify-between items-center mt-2">
                {coins.map(({ coinType, balance }) => (
                    <Link
                        to={`/send?${new URLSearchParams({
                            type: coinType,
                        }).toString()}`}
                        key={coinType}
                        className="py-3.75 px-1.5 no-underline flex gap-2 items-center w-full hover:bg-sui/10 group border-t border-solid border-transparent border-t-gray-45"
                    >
                        <CoinItem coinType={coinType} balance={balance} />
                    </Link>
                ))}
            </div>
        </div>
    );

    return (
        <div className="flex w-full">
            {showActiveCoin
                ? activeCoin && (
                      <SelectedCoinCard
                          coinType={activeCoin.coinType}
                          balance={activeCoin.balance}
                      />
                  )
                : CoinListCard}
        </div>
    );
}
