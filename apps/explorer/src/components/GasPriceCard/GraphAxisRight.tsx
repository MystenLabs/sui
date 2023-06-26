// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type scaleLinear } from '@visx/scale';
import clsx from 'clsx';

import { type UnitsType } from './types';
import { useGasPriceFormat } from './utils';

function GasPriceValue({
	value,
	selectedUnit,
	scale,
	left,
}: AxisRightProps<number> & { value: number }) {
	const valueUnit = useGasPriceFormat(BigInt(value), selectedUnit);
	return (
		<text
			x={left}
			y={scale(value)}
			textAnchor="end"
			alignmentBaseline="middle"
			className="fill-steel font-sans text-subtitleSmall font-medium"
		>
			{valueUnit}
		</text>
	);
}

export type AxisRightProps<Output> = {
	left: number;
	scale: ReturnType<typeof scaleLinear<Output>>;
	selectedUnit: UnitsType;
	isHovered: boolean;
};

export function GraphAxisRight({ scale, left, selectedUnit, isHovered }: AxisRightProps<number>) {
	let ticks = Array.from(new Set(scale.nice(6).ticks(6).map(Math.floor)).values());
	return (
		<g>
			<g>
				{ticks
					.filter((_, index) => index % 2 !== 0 || ticks.length <= 3)
					.map((value) => (
						<GasPriceValue key={value} {...{ scale, left, value, selectedUnit, isHovered }} />
					))}
			</g>
			<g>
				{ticks
					.filter((_, index) => index % 2 === 0 && ticks.length > 3)
					.map((value) => (
						<circle
							key={value}
							cx={left}
							cy={scale(value)}
							r="1"
							className={clsx(
								'fill-steel transition-all ease-ease-out-cubic',
								isHovered ? 'opacity-100' : 'opacity-0',
							)}
						/>
					))}
			</g>
		</g>
	);
}
