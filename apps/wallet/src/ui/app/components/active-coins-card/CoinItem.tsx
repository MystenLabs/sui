// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { CoinIcon } from '_components/coin-icon';
import { useFormatCoin } from '_hooks';

//TODO: Get coin metadata from getCoinMetadata right now it's hardcoded because the metadata for most other cointype is undefined
export function CoinItem({
    coinType,
    balance,
}: {
    coinType: string;
    balance: bigint;
}) {
    const [formatted, symbol] = useFormatCoin(balance, coinType);

    return (
        <div className="flex gap-2.5 w-full justify-center items-center">
            <CoinIcon coinType={coinType} size="md" />
            <div className="flex flex-1 gap-1.5 justify-between">
                <div className="flex flex-col gap-1.5">
                    <Text variant="body" color="gray-90" weight="semibold">
                        {symbol}
                    </Text>
                    <Text
                        variant="subtitle"
                        color="steel-dark"
                        weight="semibold"
                    >
                        {symbol}
                    </Text>
                </div>
                <div className="flex flex-row justify-center items-center">
                    <Text variant="body" color="gray-90" weight="semibold">
                        {formatted} {symbol}
                    </Text>
                </div>
            </div>
        </div>
    );
}
