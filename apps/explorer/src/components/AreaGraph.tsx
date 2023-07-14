// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom, AxisLeft, type TickRendererProps } from '@visx/axis';
import { curveCatmullRom as curve } from '@visx/curve';
import { localPoint } from '@visx/event';
import { PatternCircles } from '@visx/pattern';
import { scaleLinear } from '@visx/scale';
import { AreaClosed, LinePath } from '@visx/shape';
import { useTooltipInPortal, useTooltip } from '@visx/tooltip';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { throttle } from 'throttle-debounce';

import { GraphTooltipContent } from './GraphTooltipContent';

let idCounter = 0;

function getID(prefix: string) {
	return `${prefix}_${idCounter++}`;
}

const bisectX = bisector((x: number) => x).center;

function AxisLeftTick({ x, y, formattedValue }: TickRendererProps) {
	return (
		<text
			x={x}
			y={y}
			textAnchor="start"
			alignmentBaseline="middle"
			className="fill-steel font-sans text-subtitleSmall font-medium"
		>
			{formattedValue}
		</text>
	);
}

function AxisBottomTick({ x, y, formattedValue }: TickRendererProps) {
	return (
		<text
			x={x}
			y={y}
			textAnchor="middle"
			className="fill-steel font-sans text-subtitleSmall font-medium"
		>
			{formattedValue}
		</text>
	);
}

export type AreaGraphProps<D> = {
	data: D[];
	width: number;
	height: number;
	getX: (element: D) => number;
	getY: (element: D) => number;
	formatX?: (x: number) => string;
	formatY?: (y: number) => string;
	color: 'blue' | 'yellow';
	tooltipContent?: (props: { data: D }) => ReactNode;
};

export function AreaGraph<D>({
	data,
	width,
	height,
	getX,
	getY,
	formatX,
	formatY,
	color,
	tooltipContent,
}: AreaGraphProps<D>) {
	const graphTop = 15;
	const graphBottom = Math.max(0, height - 30);
	const graphLeft = 45;
	const graphRight = Math.max(0, width - 15);
	const [fillGradientID] = useState(() => getID('areaGraph_fillGradient'));
	const [lineGradientID] = useState(() => getID('areaGraph_lineGradient'));
	const [patternID] = useState(() => getID('areaGraph_pattern'));
	const { TooltipInPortal, containerRef } = useTooltipInPortal({
		scroll: true,
	});
	const { tooltipOpen, hideTooltip, showTooltip, tooltipData, tooltipLeft, tooltipTop } =
		useTooltip<D>({
			tooltipLeft: 0,
			tooltipTop: 0,
		});
	const xScale = useMemo(
		() =>
			scaleLinear<number>({
				domain: extent(data, getX) as [number, number],
				range: [graphLeft, graphRight],
			}),
		[data, graphRight, graphLeft, getX],
	);
	const yScale = useMemo(() => {
		const [min, max] = extent(data, getY) as [number, number];
		return scaleLinear<number>({
			domain: [min - min * 0.3, max],
			range: [graphBottom, graphTop],
			nice: true,
		});
	}, [data, graphTop, graphBottom, getY]);
	const handleTooltip = useCallback(
		(x: number) => {
			if (!tooltipContent) {
				return;
			}
			const selectedData = data[bisectX(data.map(getX), xScale.invert(x), 0)];
			showTooltip({
				tooltipData: selectedData,
				tooltipLeft: xScale(getX(selectedData)),
				tooltipTop: yScale(getY(selectedData)),
			});
		},
		[xScale, yScale, showTooltip, data, getX, getY, tooltipContent],
	);
	const [handleTooltipThrottled, setHandleTooltipThrottled] =
		useState<ReturnType<typeof throttle>>();
	const handleTooltipThrottledRef = useRef<ReturnType<typeof throttle>>();
	useEffect(() => {
		handleTooltipThrottledRef.current = throttle(100, handleTooltip);
		setHandleTooltipThrottled(() => handleTooltipThrottledRef.current);
		return () => {
			handleTooltipThrottledRef?.current?.cancel?.();
		};
	}, [handleTooltip]);
	const tooltipContentProps = useMemo(
		() => (tooltipData ? { data: tooltipData } : null),
		[tooltipData],
	);
	if (width < 100 || height < 100) {
		return null;
	}
	const tooltipTopAdj = tooltipTop ? Math.max(tooltipTop - 20, 0) : undefined;
	return (
		<div className="relative h-full w-full overflow-hidden" ref={containerRef}>
			{tooltipOpen && tooltipContentProps && tooltipContent ? (
				<TooltipInPortal
					key={Math.random()} // needed for bounds to update correctly
					offsetLeft={0}
					offsetTop={0}
					left={tooltipLeft}
					top={tooltipTopAdj}
					className="pointer-events-none absolute z-10 h-0 w-max overflow-visible"
					unstyled
					detectBounds
				>
					<GraphTooltipContent>{tooltipContent(tooltipContentProps)}</GraphTooltipContent>
				</TooltipInPortal>
			) : null}
			<svg width={width} height={height}>
				<defs>
					<linearGradient id={fillGradientID} gradientTransform="rotate(90)">
						{color === 'yellow' ? (
							<>
								<stop stopColor="#F2BD24" />
								<stop offset="40%" stopColor="#F2BD24" stopOpacity="40%" />
								<stop offset="100%" stopColor="#FFF8E2" stopOpacity="0%" />
							</>
						) : (
							<>
								<stop stopColor="#00F9FB" />
								<stop offset="40%" stopColor="#7CE7FF" stopOpacity="28%" />
								<stop offset="100%" stopColor="#FBF1FD" stopOpacity="0%" />
							</>
						)}
					</linearGradient>
					<linearGradient id={lineGradientID}>
						{color === 'yellow' ? (
							<>
								<stop stopColor="#8D6E15" />
								<stop offset="100%" stopColor="#F2BD24" />
							</>
						) : (
							<>
								<stop stopColor="#008BE9" />
								<stop offset="100%" stopColor="#00EEAC" />
							</>
						)}
					</linearGradient>
				</defs>
				<PatternCircles
					id={patternID}
					height={5}
					width={5}
					radius={1}
					fill="#000"
					className="opacity-5"
				/>
				<AreaClosed<D>
					curve={curve}
					data={data}
					yScale={yScale}
					x={(d) => xScale(getX(d))}
					y={(d) => yScale(getY(d))}
					fill={`url(#${fillGradientID})`}
					stroke="transparent"
				/>
				<AreaClosed<D>
					curve={curve}
					data={data}
					yScale={yScale}
					x={(d) => xScale(getX(d))}
					y={(d) => yScale(getY(d))}
					fill={`url(#${patternID})`}
					stroke="transparent"
				/>
				<LinePath<D>
					curve={curve}
					data={data}
					x={(d) => xScale(getX(d))}
					y={(d) => yScale(getY(d))}
					stroke={`url(#${lineGradientID})`}
					width="1"
				/>
				<AxisBottom
					top={height - 24}
					orientation="bottom"
					scale={xScale}
					tickFormat={formatX ? (x) => formatX(x.valueOf()) : String}
					hideTicks
					hideAxisLine
					tickValues={xScale
						.ticks(Math.min(data.length, Math.floor((width - 50) / 40)))
						.filter(Number.isInteger)}
					tickComponent={AxisBottomTick}
				/>
				<AxisLeft
					left={10}
					orientation="left"
					scale={yScale}
					tickFormat={formatY ? (y) => formatY(y.valueOf()) : String}
					hideTicks
					hideAxisLine
					tickValues={yScale.ticks(4).filter(Number.isInteger)}
					tickComponent={AxisLeftTick}
				/>
				{tooltipContent ? (
					<>
						<line
							x1={0}
							y1={tooltipTopAdj}
							x2={0}
							y2={height - 20}
							className={clsx('stroke-steel/40', tooltipOpen ? 'opacity-100' : 'opacity-0')}
							strokeWidth="1"
							transform={tooltipLeft ? `translate(${tooltipLeft})` : ''}
						/>
						<line
							x1={graphLeft - 5}
							y1={0}
							x2={graphRight}
							y2={0}
							className={clsx('stroke-steel/40', tooltipOpen ? 'opacity-100' : 'opacity-0')}
							strokeWidth="1"
							transform={tooltipTop ? `translate(0, ${tooltipTop})` : ''}
						/>
					</>
				) : null}
				{tooltipContent ? (
					<rect
						x={0}
						y={0}
						width={width}
						height={height}
						fill="transparent"
						stroke="none"
						onMouseEnter={(e) => {
							handleTooltipThrottled?.(localPoint(e)?.x || graphLeft);
						}}
						onMouseMove={(e) => {
							handleTooltipThrottled?.(localPoint(e)?.x || graphLeft);
						}}
						onMouseLeave={() => {
							handleTooltipThrottled?.cancel({
								upcomingOnly: true,
							});
							hideTooltip();
						}}
					/>
				) : null}
			</svg>
		</div>
	);
}
