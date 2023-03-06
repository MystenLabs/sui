// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { CoinItem } from './CoinItem';
import { Text } from '_app/shared/text';
import { useAppSelector } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';

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

    const activeCoin = coins.find(
        ({ coinType }) => coinType === activeCoinType
    );

    return (
        <div className="flex w-full">
            {showActiveCoin ? (
                activeCoin && (
                    <Link
                        to={`/send/select?${new URLSearchParams({
                            type: activeCoin.coinType,
                        }).toString()}`}
                        className="bg-gray-40 rounded-2lg py-2.5 px-3 no-underline flex gap-2 items-center w-full"
                    >
                        <CoinItem
                            coinType={activeCoin.coinType}
                            balance={activeCoin.balance}
                            isActive
                        />
                    </Link>
                )
            ) : (
                <div className="flex flex-col w-full">
                    <Text
                        variant="caption"
                        color="steel-darker"
                        weight="semibold"
                    >
                        My Coins
                    </Text>
                    <div className="flex flex-col justify-between items-center mt-2">
                        {coins?.map(({ coinType, balance }) => (
                            <Link
                                to={`/send?${new URLSearchParams({
                                    type: coinType,
                                }).toString()}`}
                                key={coinType}
                                className="py-3.75 px-1.5 no-underline flex gap-2 items-center w-full hover:bg-sui/10 group border-t border-solid border-transparent border-t-gray-45"
                            >
                                <CoinItem
                                    coinType={coinType}
                                    balance={BigInt(balance)}
                                />
                            </Link>
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
}
