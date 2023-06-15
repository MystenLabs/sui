// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate } from '@mysten/core';
import { useTooltipPosition } from '@visx/tooltip';
import clsx from 'clsx';

import { type EpochGasInfo, type UnitsType } from './types';
import { useGasPriceFormat } from './utils';

import { Text } from '~/ui/Text';

export type GraphTooltipContentProps = {
	hoveredElement: EpochGasInfo;
	unit: UnitsType;
};

export function GraphTooltipContent({ hoveredElement, unit }: GraphTooltipContentProps) {
	const formattedHoveredPrice = useGasPriceFormat(hoveredElement?.referenceGasPrice ?? null, unit);
	const formattedHoveredDate = hoveredElement?.date
		? formatDate(hoveredElement?.date, ['month', 'day'])
		: '-';
	const { isFlippedHorizontally } = useTooltipPosition();
	return (
		<div
			className={clsx(
				'flex w-fit -translate-y-[calc(100%-10px)] flex-col flex-nowrap gap-0.5 whitespace-nowrap rounded-md border border-solid border-gray-45 bg-gray-90 px-2 py-1.5',
				hoveredElement?.date ? 'visible' : 'invisible',
				isFlippedHorizontally
					? '-translate-x-[1px] rounded-bl-none'
					: 'translate-x-[1px] rounded-br-none',
			)}
		>
			<Text variant="caption/semibold" color="white">
				<div className="whitespace-nowrap">
					{formattedHoveredPrice ? `${formattedHoveredPrice} ${unit}` : '-'}
				</div>
			</Text>
			<Text variant="subtitleSmallExtra/medium" color="white">
				Epoch {hoveredElement?.epoch}, {formattedHoveredDate}
			</Text>
		</div>
	);
}
