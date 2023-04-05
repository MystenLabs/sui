// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { Text } from '~/ui/Text';
import { Disclosure } from '@headlessui/react';
import { ArrowShowAndHideRight12 } from '@mysten/icons';
import CoinItem from './CoinItem';
import { CoinStruct } from '@mysten/sui.js';

type CoinViewProps = {
    coinType: string;
    objects: CoinStruct[];
};

const CoinView = ({ coinType, objects }: CoinViewProps) => {
    let totalBalance = 0;
    objects.forEach((obj) => {
        totalBalance += obj.balance;
    });

    const [formattedTotalBalance, symbol] = useFormatCoin(
        totalBalance,
        coinType
    );

    return (
        <Disclosure>
            <Disclosure.Button
                data-testid="ownedcoinlabel"
                className="grid w-full grid-cols-3 items-center justify-between rounded-none py-2 text-left hover:bg-sui-light"
            >
                <div className="flex">
                    <ArrowShowAndHideRight12 className="mr-1.5 fill-gray-60 ui-open:rotate-90 ui-open:transform" />
                    <Text color="steel-darker" variant="body/medium">
                        {symbol}
                    </Text>
                </div>

                <Text color="steel-darker" variant="body/medium">
                    {objects.length}
                </Text>

                <div className="flex items-center gap-1">
                    <Text color="steel-darker" variant="bodySmall/medium">
                        {formattedTotalBalance}
                    </Text>
                    <Text color="steel" variant="subtitleSmallExtra/normal">
                        {symbol}
                    </Text>
                </div>
            </Disclosure.Button>

            <Disclosure.Panel>
                <div className="flex flex-col gap-1 bg-gray-40 p-3">
                    {objects.map((coin) => (
                        <CoinItem key={coin.coinObjectId} coin={coin} />
                    ))}
                </div>
            </Disclosure.Panel>
        </Disclosure>
    );
};

export default CoinView;
