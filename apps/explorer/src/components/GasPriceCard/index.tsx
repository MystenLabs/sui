// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetReferenceGasPrice, useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { ParentSize } from '@visx/responsive';
import clsx from 'clsx';
import { useMemo, useState } from 'react';

import { ErrorBoundary } from '../error-boundary/ErrorBoundary';
import { Graph } from './Graph';
import { type EpochGasInfo, type GraphDurationsType, type UnitsType } from './types';
import { GRAPH_DURATIONS, GRAPH_DURATIONS_MAP, UNITS, isDefined, useGasPriceFormat } from './utils';

import { Card } from '~/ui/Card';
import { FilterList } from '~/ui/FilterList';
import { Heading } from '~/ui/Heading';
import { ListboxSelect } from '~/ui/ListboxSelect';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Stats } from '~/ui/Stats';
import { Text } from '~/ui/Text';

function useHistoricalGasPrices() {
	const rpc = useRpcClient();
	return useQuery<EpochGasInfo[]>({
		queryKey: ['get', 'last 30 epochs gas price'],
		queryFn: async () => {
			// TODO: update this to get the gas price from the epoch itself rather than the previous one
			// once this is done https://mysten.atlassian.net/browse/PI-6
			// currently every epoch contains the gas price for the next one
			const epochs = [
				...(
					await rpc.getEpochs({
						descendingOrder: true,
						limit: 31,
					})
				).data,
			].reverse();

			// remove the current epoch since it would have the gasPrice for the next one
			epochs.pop();
			return epochs.map((anEpoch) => ({
				epoch: Number(anEpoch.epoch) + 1,
				referenceGasPrice: anEpoch.endOfEpochInfo?.referenceGasPrice
					? BigInt(anEpoch.endOfEpochInfo?.referenceGasPrice)
					: null,
				date: anEpoch.endOfEpochInfo?.epochEndTimestamp
					? new Date(Number(anEpoch.endOfEpochInfo?.epochEndTimestamp))
					: null,
			}));
		},
	});
}

function useGasPriceAverage(totalEpochs: number) {
	const historicalData = useHistoricalGasPrices();
	const { data } = historicalData;
	const average = useMemo(() => {
		const epochs = data?.slice(-totalEpochs) || [];
		const epochsWithPrices = epochs.filter(({ referenceGasPrice }) => referenceGasPrice !== null);
		if (epochsWithPrices.length) {
			const sum = epochsWithPrices.reduce(
				(acc, { referenceGasPrice }) => acc + BigInt(referenceGasPrice!),
				0n,
			);
			return sum / BigInt(epochsWithPrices.length);
		}
		return null;
	}, [data, totalEpochs]);
	return { ...historicalData, data: average };
}

// TODO: Delete this prop once we roll out the SUI token card
export function GasPriceCard({ useLargeSpacing }: { useLargeSpacing: boolean }) {
	const [selectedUnit, setSelectedUnit] = useState<UnitsType>(UNITS[0]);
	// use this to show current gas price for envs that historical data is not available
	const { data: backupCurrentEpochGasPrice, isLoading: isCurrentLoading } =
		useGetReferenceGasPrice();
	const { data: average7Epochs, isLoading: isAverage7EpochsLoading } = useGasPriceAverage(7);
	const { data: historicalData, isLoading: isHistoricalLoading } = useHistoricalGasPrices();
	const isDataLoading = isHistoricalLoading || isCurrentLoading;
	const lastGasPriceInHistoricalData = useMemo(
		() => historicalData?.filter(isDefined).pop()?.referenceGasPrice ?? null,
		[historicalData],
	);
	const formattedCurrentGasPrice = useGasPriceFormat(
		isDataLoading ? null : lastGasPriceInHistoricalData ?? backupCurrentEpochGasPrice ?? null,
		selectedUnit,
	);
	const formattedAverageGasPrice = useGasPriceFormat(
		isDataLoading ? null : average7Epochs,
		selectedUnit,
	);
	const [selectedGraphDuration, setSelectedGraphsDuration] =
		useState<GraphDurationsType>('30 Epochs');
	const graphEpochs = useMemo(
		() => historicalData?.slice(-GRAPH_DURATIONS_MAP[selectedGraphDuration]) || [],
		[historicalData, selectedGraphDuration],
	);
	return (
		<Card spacing="none" height="full" bg="white" border="gray45">
			<div className={clsx('flex h-full flex-col', useLargeSpacing ? 'gap-8' : 'gap-5')}>
				<div className="flex items-center gap-2.5 p-6 pb-0">
					<div className="flex flex-grow flex-wrap items-center gap-2 text-steel">
						<Heading variant="heading4/semibold" color="steel-darker">
							Reference Gas Price
						</Heading>
						<ListboxSelect
							value={selectedGraphDuration}
							options={GRAPH_DURATIONS}
							onSelect={setSelectedGraphsDuration}
						/>
					</div>
					<FilterList<UnitsType>
						lessSpacing
						size="sm"
						options={UNITS}
						value={selectedUnit}
						onChange={setSelectedUnit}
					/>
				</div>
				<div className="flex flex-wrap gap-6 px-6 lg:max-xl:gap-12">
					<Stats label="Current" postfix={selectedUnit} size="sm">
						{formattedCurrentGasPrice}
					</Stats>
					{isAverage7EpochsLoading || formattedAverageGasPrice ? (
						<Stats label="7 epochs avg" postfix={selectedUnit} size="sm">
							{formattedAverageGasPrice}
						</Stats>
					) : null}
				</div>
				<div
					className={clsx(
						'flex min-h-[180px] flex-1 flex-col items-center justify-center rounded-b-xl transition-colors',
						!graphEpochs?.length && 'bg-gray-40',
					)}
				>
					{isDataLoading ? (
						<div className="flex flex-col items-center gap-1">
							<LoadingSpinner />
							<Text color="steel" variant="body/medium">
								loading data
							</Text>
						</div>
					) : graphEpochs?.length ? (
						<div className="relative flex-1 self-stretch">
							<ErrorBoundary>
								<ParentSize className="absolute">
									{(parent) => (
										<Graph
											width={parent.width}
											height={parent.height}
											data={graphEpochs}
											selectedUnit={selectedUnit}
										/>
									)}
								</ParentSize>
							</ErrorBoundary>
						</div>
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
