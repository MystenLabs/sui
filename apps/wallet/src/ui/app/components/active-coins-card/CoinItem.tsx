// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import Loading from '../loading';
import { Text } from '_app/shared/text';
import { CoinIcon } from '_components/coin-icon';
import { useRpc, useFormatCoin } from '_hooks';

// Get coin metadata from the coin type
export function CoinItem({
    coinType,
    balance,
}: {
    coinType: string;
    balance: bigint;
}) {
    const rpc = useRpc();

    // Get coin metadata from the coin type
    // Include icon url and name of the coin
    const { data, isLoading } = useQuery(
        ['get-coin-meta', coinType],
        async () => {
            return rpc.getCoinMetadata(coinType);
        },
        { enabled: !!coinType }
    );

    const [formatted, symbol] = useFormatCoin(balance, coinType);

    return (
        <Loading loading={isLoading}>
            <div className="flex gap-2.5 w-full" role="button">
                <CoinIcon coinType={coinType} size="md" />
                <div className="flex flex-col flex-1 gap-1.5">
                    <div className="flex flex-row justify-between">
                        <Text variant="body" color="gray-90" weight="semibold">
                            {data?.name || symbol}
                        </Text>

                        <Text variant="body" color="gray-90" weight="semibold">
                            {formatted} {symbol}
                        </Text>
                    </div>
                    <div className="flex flex-row">
                        <Text
                            variant="subtitle"
                            color="steel-dark"
                            weight="semibold"
                        >
                            {symbol}
                        </Text>
                    </div>
                </div>
            </div>
        </Loading>
    );
}
