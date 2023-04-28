// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    CoinFormat,
    formatBalance,
    formatDate,
    useCoinDecimals,
    useGetReferenceGasPrice,
    useRpcClient,
} from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { ParentSize } from '@visx/responsive';
import clsx from 'clsx';
import { useMemo, useState } from 'react';

import { ErrorBoundary } from '../error-boundary/ErrorBoundary';
import { Graph, isDefined } from './Graph';
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
            // TODO: update this to get the gas price from the epoch itself rather than the previous one
            // once this is deployed https://github.com/MystenLabs/sui/pull/11388
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
    const historicalData = useHistoricalGasPrices();
    const { data } = historicalData;
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
    return { ...historicalData, data: average };
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
    // use this to show current gas price for envs that historical data is not available
    const { data: backupCurrentEpochGasPrice, isLoading: isCurrentLoading } =
        useGetReferenceGasPrice();
    const { data: average7Epochs, isLoading: isAverage7EpochsLoading } =
        useGasPriceAverage(7);
    const { data: historicalData, isLoading: isHistoricalLoading } =
        useHistoricalGasPrices();
    const isDataLoading = isHistoricalLoading || isCurrentLoading;
    const lastGasPriceInHistoricalData = useMemo(
        () =>
            historicalData?.filter(isDefined).pop()?.referenceGasPrice ?? null,
        [historicalData]
    );
    const formattedCurrentGasPrice = useGasPriceFormat(
        isDataLoading
            ? null
            : lastGasPriceInHistoricalData ??
                  backupCurrentEpochGasPrice ??
                  null,
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
    const [hoveredElement, setHoveredElement] = useState<EpochGasInfo | null>(
        null
    );
    const formattedHoveredPrice = useGasPriceFormat(
        hoveredElement?.referenceGasPrice ?? null,
        selectedUnit
    );
    const formattedHoveredDate = hoveredElement?.date
        ? formatDate(hoveredElement?.date, ['month', 'day', 'year'])
        : '-';
    return (
        <Card spacing="lg">
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
                <div className="flex min-h-[180px] flex-1 flex-col items-center justify-center overflow-hidden rounded-xl bg-white pt-2">
                    {isDataLoading ? (
                        <div className="flex flex-col items-center gap-1">
                            <LoadingSpinner />
                            <Text color="steel" variant="body/medium">
                                loading data
                            </Text>
                        </div>
                    ) : historicalData?.length ? (
                        <>
                            <div className="flex flex-row self-stretch pr-2">
                                <div
                                    className={clsx(
                                        'ml-3 mt-1 flex min-w-0 flex-col flex-nowrap gap-0.5 rounded-md border border-solid border-gray-45 px-2 py-1.5',
                                        hoveredElement?.date
                                            ? 'visible'
                                            : 'invisible'
                                    )}
                                >
                                    <Text
                                        variant="caption/semibold"
                                        color="hero-dark"
                                        truncate
                                    >
                                        {formattedHoveredPrice
                                            ? `${formattedHoveredPrice} ${selectedUnit}`
                                            : '-'}
                                    </Text>
                                    <Text
                                        variant="subtitleSmallExtra/medium"
                                        color="steel-darker"
                                    >
                                        {formattedHoveredDate}
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
                                <ErrorBoundary>
                                    {historicalData ? (
                                        <ParentSize className="absolute">
                                            {(parent) => (
                                                <Graph
                                                    width={parent.width}
                                                    height={parent.height}
                                                    data={graphEpochs}
                                                    onHoverElement={
                                                        setHoveredElement
                                                    }
                                                />
                                            )}
                                        </ParentSize>
                                    ) : null}
                                </ErrorBoundary>
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
