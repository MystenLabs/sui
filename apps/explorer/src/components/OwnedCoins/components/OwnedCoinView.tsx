// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { useFormatCoin, useRpcClient } from '@mysten/core';
import { ArrowShowAndHideRight12 } from '@mysten/icons';
import { CoinStruct, type CoinBalance } from '@mysten/sui.js';

import { Text } from '~/ui/Text';
import { useContext, useEffect, useState } from 'react';
import { AddressContext } from '~/pages/address-result/AddressResult';
import CoinsPanel from './OwnedCoinsPanel';

type OwnedCoinViewProps = {
    coin: CoinBalance;
};

function OwnedCoinView({ coin }: OwnedCoinViewProps) {
    const [coinObjects, setCoinObjects] = useState<CoinStruct[]>([])
    const [hasNextPage, setHasNextPage] = useState<boolean>(false)
    const [nextCursor, setNextCursor] = useState<string | null>()
    const ownerId = useContext(AddressContext);
    const rpc = useRpcClient();
    const [formattedTotalBalance, symbol] = useFormatCoin(
        coin.totalBalance,
        coin.coinType
    );

    const getCoins = async (nextCursor?: string) => {
        return rpc.getCoins({ owner: ownerId, coinType: coin.coinType, limit: 10, cursor: nextCursor }).then((resp) => {
            nextCursor && console.log(resp.data)
            setCoinObjects([...coinObjects, ...resp.data]);
            setHasNextPage(resp.hasNextPage);
            setNextCursor(resp.nextCursor);
        })
    }

    useEffect(() => {
       getCoins()
    }, [ownerId, rpc]);

    return (
        <Disclosure>
            <Disclosure.Button
                data-testid="ownedcoinlabel"
                className="grid w-full grid-cols-3 items-center justify-between rounded-none py-2 text-left hover:bg-sui-light"
            >
                <div className="flex">
                    <ArrowShowAndHideRight12 className="mr-1.5 text-gray-60 ui-open:rotate-90 ui-open:transform" />
                    <Text color="steel-darker" variant="body/medium">
                        {symbol}
                    </Text>
                </div>

                <Text color="steel-darker" variant="body/medium">
                    {coin.coinObjectCount}
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
                    <CoinsPanel coins={coinObjects} 
                    fetchCoins={getCoins} 
                    nextCursor={nextCursor} 
                    hasNextPage={hasNextPage} />
                </div>
            </Disclosure.Panel>
        </Disclosure>
    );
}

export default OwnedCoinView