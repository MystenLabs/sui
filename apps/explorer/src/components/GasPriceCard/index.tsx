// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    CoinFormat,
    formatBalance,
    useCoinDecimals,
    useGetReferenceGasPrice,
    useRpcClient,
} from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { ParentSize } from '@visx/responsive';
import { useMemo, useState } from 'react';

import { Graph } from './Graph';
import { type EpochGasInfo } from './types';

import { Card } from '~/ui/Card';
import { FilterList } from '~/ui/FilterList';
import { Heading } from '~/ui/Heading';
import { ListboxSelect } from '~/ui/ListboxSelect';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Stats } from '~/ui/Stats';
import { Text } from '~/ui/Text';

const UNITS = ['MIST', 'SUI'] as const;
type UnitsType = (typeof UNITS)[number];
const GRAPH_DURATIONS = ['7 EPOCHS', '30 EPOCHS'] as const;
type GraphDurationsType = (typeof GRAPH_DURATIONS)[number];
const GRAPH_DURATIONS_MAP: Record<GraphDurationsType, number> = {
    '7 EPOCHS': 7,
    '30 EPOCHS': 30,
};

function useHistoricalGasPrices() {
    const rpc = useRpcClient();
    return useQuery<EpochGasInfo[]>(
        ['get', 'last 30 epochs gas price'],
        async () => {
            // every epoch contains the gas price for the next one
            const epochs = (
                await rpc.getEpochs({
                    descendingOrder: true,
                    limit: 31,
                })
            ).data.reverse();
            // remove the current epoch since it would have the gasPrice for the next one
            epochs.pop();
            return epochs.map((anEpoch) => ({
                epoch: Number(anEpoch.epoch) + 1,
                referenceGasPrice: anEpoch.endOfEpochInfo?.referenceGasPrice
                    ? BigInt(anEpoch.endOfEpochInfo?.referenceGasPrice)
                    : null,
                date: anEpoch.endOfEpochInfo?.epochEndTimestamp
                    ? new Date(
                          Number(anEpoch.endOfEpochInfo?.epochEndTimestamp)
                      )
                    : null,
            }));
        }
    );
}

function useGasPriceAverage(totalEpochs: number) {
    const { data, isLoading } = useHistoricalGasPrices();
    const average = useMemo(() => {
        const epochs = data?.slice(-totalEpochs) || [];
        const epochsWithPrices = epochs.filter(
            ({ referenceGasPrice }) => referenceGasPrice !== null
        );
        if (epochsWithPrices.length) {
            const sum = epochsWithPrices.reduce(
                (acc, { referenceGasPrice }) =>
                    acc + BigInt(referenceGasPrice!),
                0n
            );
            return sum / BigInt(epochsWithPrices.length);
        }
        return null;
    }, [data, totalEpochs]);
    return [average, isLoading] as const;
}

function useGasPriceFormat(gasPrice: bigint | null, unit: 'MIST' | 'SUI') {
    const [suiDecimals] = useCoinDecimals(SUI_TYPE_ARG);
    return gasPrice !== null
        ? formatBalance(
              gasPrice,
              unit === 'MIST' ? 0 : suiDecimals,
              CoinFormat.FULL
          )
        : null;
}

export function GasPriceCard() {
    const [selectedUnit, setSelectedUnit] = useState<UnitsType>(UNITS[0]);
    const { data: currentEpochGasPrice, isLoading: isCurrentLoading } =
        useGetReferenceGasPrice();
    const [average7Epochs, isAverage7EpochsLoading] = useGasPriceAverage(7);
    const { data: historicalData, isLoading: isHistoricalLoading } =
        useHistoricalGasPrices();
    const isDataLoading = isHistoricalLoading || isCurrentLoading;
    const formattedCurrentGasPrice = useGasPriceFormat(
        isDataLoading ? null : currentEpochGasPrice ?? null,
        selectedUnit
    );
    const formattedAverageGasPrice = useGasPriceFormat(
        isDataLoading ? null : average7Epochs,
        selectedUnit
    );
    const [selectedGraphDuration, setSelectedGraphsDuration] =
        useState<GraphDurationsType>('7 EPOCHS');
    const graphEpochs = useMemo(
        () =>
            historicalData?.slice(
                -GRAPH_DURATIONS_MAP[selectedGraphDuration]
            ) || [],
        [historicalData, selectedGraphDuration]
    );
    return (
        <Card bg="default" spacing="lg">
            <div className="flex flex-col gap-5">
                <div className="flex gap-2.5">
                    <div className="flex-grow">
                        <Heading
                            variant="heading4/semibold"
                            color="steel-darker"
                        >
                            Gas Price
                        </Heading>
                    </div>
                    <FilterList<UnitsType>
                        lessSpacing
                        size="sm"
                        options={UNITS}
                        value={selectedUnit}
                        onChange={setSelectedUnit}
                    />
                </div>
                <div className="flex gap-6">
                    <Stats label="Current" postfix={selectedUnit}>
                        {formattedCurrentGasPrice}
                    </Stats>
                    {isAverage7EpochsLoading || formattedAverageGasPrice ? (
                        <Stats label="7 epochs avg" postfix={selectedUnit}>
                            {formattedAverageGasPrice}
                        </Stats>
                    ) : null}
                </div>
                <div className="flex min-h-[30vh] flex-1 flex-col items-center justify-center rounded-xl bg-white pt-2">
                    {isDataLoading ? (
                        <>
                            <LoadingSpinner />
                            <Text color="steel" variant="body/medium">
                                loading data
                            </Text>
                        </>
                    ) : historicalData ? (
                        <>
                            <div className="flex flex-row self-stretch pr-2">
                                <div className="ml-3 mt-1 flex min-w-0 flex-col flex-nowrap gap-0.5 rounded-md border border-solid border-gray-45 px-2 py-1.5">
                                    <Text
                                        variant="caption/semibold"
                                        color="hero-dark"
                                        truncate
                                    >
                                        420 MIST
                                    </Text>
                                    <Text
                                        variant="subtitleSmallExtra/medium"
                                        color="steel-darker"
                                    >
                                        March 30, 2023
                                    </Text>
                                </div>
                                <div className="flex-1" />
                                <ListboxSelect
                                    value={selectedGraphDuration}
                                    options={GRAPH_DURATIONS}
                                    onSelect={setSelectedGraphsDuration}
                                />
                            </div>
                            <div className="relative flex-1 self-stretch">
                                {historicalData ? (
                                    <ParentSize className="absolute">
                                        {(parent) => (
                                            <Graph
                                                width={parent.width}
                                                height={parent.height}
                                                data={graphEpochs}
                                                durationDays={
                                                    GRAPH_DURATIONS_MAP[
                                                        selectedGraphDuration
                                                    ]
                                                }
                                            />
                                        )}
                                    </ParentSize>
                                ) : null}
                            </div>
                        </>
                    ) : (
                        <Text color="steel" variant="body/medium">
                            No historical data available
                        </Text>
                    )}
                </div>
            </div>
        </Card>
    );
}
