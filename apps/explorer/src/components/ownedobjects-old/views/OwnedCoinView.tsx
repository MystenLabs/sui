// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { useFormatCoin } from '@mysten/core';
import { Coin } from '@mysten/sui.js';
import { useState } from 'react';

import { ReactComponent as ClosedIcon } from '../../../assets/SVGIcons/12px/ShowNHideRight.svg';
import Pagination from '../../pagination/Pagination';
import { type DataType, ITEMS_PER_PAGE } from '../OwnedObjectConstants';

import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';

function CoinItem({
    id,
    balance,
    coinType,
}: {
    id: string;
    balance?: bigint | null;
    coinType?: string | null;
}) {
    const [formattedBalance, symbol] = useFormatCoin(balance, coinType);

    return (
        <div className="bg-grey-40 grid grid-flow-row auto-rows-fr grid-cols-4 items-center">
            <Text color="steel-darker" variant="bodySmall/medium">
                Object ID
            </Text>
            <div className="col-span-3">
                <ObjectLink objectId={id} noTruncate />
            </div>

            <Text color="steel-darker" variant="bodySmall/medium">
                Balance
            </Text>

            <div className="col-span-3 inline-flex items-end gap-1">
                <Text color="steel-darker" variant="bodySmall/medium">
                    {formattedBalance}
                </Text>
                <Text color="steel" variant="subtitleSmallExtra/normal">
                    {symbol}
                </Text>
            </div>
        </div>
    );
}

function SingleCoinView({
    results,
    coinType,
    currentPage,
}: {
    results: DataType;
    coinType: string;
    currentPage: number;
}) {
    const subObjList = results.filter(({ Type }) => Type === coinType);

    const totalBalance =
        subObjList[0]._isCoin &&
        subObjList.every((el) => el.balance !== undefined)
            ? subObjList.reduce((prev, current) => prev + current.balance!, 0n)
            : null;

    const extractedCoinType = Coin.getCoinType(coinType);

    const [formattedTotalBalance, symbol] = useFormatCoin(
        totalBalance,
        extractedCoinType
    );

    return (
        <Disclosure>
            <Disclosure.Button
                data-testid="ownedcoinlabel"
                className="grid w-full grid-cols-3 items-center justify-between rounded-none py-2 text-left hover:bg-sui-light"
            >
                <div className="flex">
                    <ClosedIcon className="mr-1.5 fill-gray-60 ui-open:rotate-90 ui-open:transform" />
                    <Text color="steel-darker" variant="body/medium">
                        {symbol}
                    </Text>
                </div>

                <Text color="steel-darker" variant="body/medium">
                    {subObjList.length}
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
                    {subObjList.map((subObj) => (
                        <CoinItem
                            key={subObj.id}
                            id={subObj.id}
                            coinType={extractedCoinType}
                            balance={subObj.balance}
                        />
                    ))}
                </div>
            </Disclosure.Panel>
        </Disclosure>
    );
}

export default function OwnedCoinView({ results }: { results: DataType }) {
    const [currentPage, setCurrentPage] = useState(1);

    const uniqueTypes = Array.from(new Set(results.map(({ Type }) => Type)));

    return (
        <div className="flex flex-col text-left">
            <div className="flex max-h-80 flex-col overflow-auto">
                <div className="grid grid-cols-3 py-2 uppercase tracking-wider text-gray-80">
                    <Text variant="caption/medium">Type</Text>
                    <Text variant="caption/medium">Objects</Text>
                    <Text variant="caption/medium">Balance</Text>
                </div>
                <div>
                    {uniqueTypes
                        .slice(
                            (currentPage - 1) * ITEMS_PER_PAGE,
                            currentPage * ITEMS_PER_PAGE
                        )
                        .map((type) => (
                            <SingleCoinView
                                key={type}
                                results={results}
                                coinType={type}
                                currentPage={currentPage}
                            />
                        ))}
                </div>
            </div>
            <Pagination
                totalItems={uniqueTypes.length}
                itemsPerPage={ITEMS_PER_PAGE}
                currentPage={currentPage}
                onPagiChangeFn={setCurrentPage}
            />
        </div>
    );
}
