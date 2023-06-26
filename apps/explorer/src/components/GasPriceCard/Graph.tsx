// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom } from '@visx/axis';
import { curveLinear } from '@visx/curve';
import { localPoint } from '@visx/event';
import { MarkerCircle } from '@visx/marker';
import { scaleLinear } from '@visx/scale';
import { AreaClosed, LinePath } from '@visx/shape';
import { useTooltip, useTooltipInPortal } from '@visx/tooltip';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';

import { GraphAxisRight } from './GraphAxisRight';
import { GraphTooltipContent } from './GraphTooltipContent';
import { type UnitsType, type EpochGasInfo } from './types';
import { isDefined } from './utils';

const SIDE_MARGIN = 15;

const bisectEpoch = bisector(({ epoch }: EpochGasInfo) => epoch).center;

export type GraphProps = {
	data: EpochGasInfo[];
	selectedUnit: UnitsType;
	width: number;
	height: number;
};
export function Graph({ data, width, height, selectedUnit }: GraphProps) {
	// remove not defined data (graph displays better and helps with hovering/selecting hovered element)
	const adjData = useMemo(() => data.filter(isDefined), [data]);
	const graphTop = 10;
	const graphButton = Math.max(height - 45, 0);
	const xScale = useMemo(
		() =>
			scaleLinear<number>({
				domain: extent(adjData, ({ epoch }) => epoch) as [number, number],
				range: [SIDE_MARGIN, width - SIDE_MARGIN],
			}),
		[width, adjData],
	);
	const yScale = useMemo(
		() =>
			scaleLinear<number>({
				domain: extent(adjData, ({ referenceGasPrice }) =>
					referenceGasPrice !== null ? Number(referenceGasPrice) : null,
				) as number[],
				range: [graphButton, graphTop],
			}),
		[adjData, graphTop, graphButton],
	);
	const { TooltipInPortal, containerRef } = useTooltipInPortal({
		scroll: true,
	});
	const { tooltipOpen, hideTooltip, showTooltip, tooltipData, tooltipLeft, tooltipTop } =
		useTooltip<EpochGasInfo>({
			tooltipLeft: 0,
			tooltipTop: 0,
		});
	const handleTooltip = useCallback(
		(x: number) => {
			const xEpoch = xScale.invert(x);
			const epochIndex = bisectEpoch(adjData, xEpoch, 0);
			const selectedEpoch = adjData[epochIndex];
			showTooltip({
				tooltipData: selectedEpoch,
				tooltipLeft: xScale(selectedEpoch.epoch),
				tooltipTop: yScale(Number(selectedEpoch.referenceGasPrice)),
			});
		},
		[xScale, adjData, yScale, showTooltip],
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
	// calculates the total ticks of 4 (~30px + some margin) digit epochs that could fit
	const totalMaxTicksForWidth = Math.floor((width - 2 * SIDE_MARGIN) / 34);
	const totalTicks = Math.min(
		adjData.length,
		totalMaxTicksForWidth < 1 ? 1 : totalMaxTicksForWidth,
	);
	const firstElementY = adjData.length ? yScale(Number(adjData[0].referenceGasPrice)) : null;
	const lastElementX = adjData.length ? xScale(adjData[adjData.length - 1].epoch) : null;
	const lastElementY = adjData.length
		? yScale(Number(adjData[adjData.length - 1].referenceGasPrice))
		: null;
	if (height < 60 || width < 100) {
		return null;
	}
	return (
		<div className="relative h-full w-full overflow-hidden" ref={containerRef}>
			{tooltipOpen && tooltipData ? (
				<TooltipInPortal
					key={Math.random()} // needed for bounds to update correctly
					offsetLeft={0}
					offsetTop={0}
					left={tooltipLeft}
					top={0}
					className="pointer-events-none absolute z-10 h-0 w-max overflow-visible"
					unstyled
					detectBounds
				>
					<GraphTooltipContent hoveredElement={tooltipData!} unit={selectedUnit} />
				</TooltipInPortal>
			) : null}
			<svg width={width} height={height}>
				<line
					x1={0}
					y1={Math.max(graphTop - 5, 0)}
					x2={0}
					y2={Math.max(height - 20, 0)}
					className={clsx('stroke-steel/30', tooltipOpen ? 'opacity-100' : 'opacity-0')}
					strokeWidth="1"
					transform={tooltipLeft ? `translate(${tooltipLeft})` : ''}
				/>
				<line
					x1={0}
					y1={0}
					x2={width}
					y2={0}
					className={clsx('stroke-steel/30', tooltipOpen ? 'opacity-100' : 'opacity-0')}
					strokeWidth="1"
					transform={tooltipTop ? `translate(0, ${tooltipTop})` : ''}
				/>
				<LinePath<EpochGasInfo>
					curve={curveLinear}
					data={adjData}
					x={(d) => xScale(d.epoch)}
					y={(d) => yScale(Number(d.referenceGasPrice!))}
					stroke="#F2BD24"
					width="1"
					fillRule="nonzero"
				/>
				<AreaClosed<EpochGasInfo>
					curve={curveLinear}
					data={adjData}
					yScale={yScale}
					x={(d) => xScale(d.epoch)}
					y={(d) => yScale(Number(d.referenceGasPrice!))}
					fill="#F2BD24"
					fillOpacity="0.1"
					stroke="transparent"
				/>
				{firstElementY !== null ? (
					<>
						<rect
							x="0"
							y={firstElementY}
							width={SIDE_MARGIN}
							fill="#F2BD24"
							fillOpacity="0.1"
							stroke="transparent"
							height={graphButton - firstElementY}
						/>
						<line x1="0" y1={firstElementY} x2={SIDE_MARGIN} y2={firstElementY} stroke="#F2BD24" />
					</>
				) : null}
				{lastElementX !== null && lastElementY !== null ? (
					<>
						<rect
							x={lastElementX}
							y={lastElementY}
							width={SIDE_MARGIN}
							fill="#F2BD24"
							fillOpacity="0.1"
							stroke="transparent"
							height={graphButton - lastElementY}
						/>
						<line
							x1={lastElementX}
							y1={lastElementY}
							x2={lastElementX + SIDE_MARGIN}
							y2={lastElementY}
							stroke="#F2BD24"
						/>
					</>
				) : null}
				<rect
					x={0}
					y={graphButton}
					width={width}
					height={Math.max(height - graphButton, 0)}
					fill="#F2BD24"
					fillOpacity="0.1"
					stroke="none"
				/>
				<GraphAxisRight
					left={width - SIDE_MARGIN / 2}
					scale={yScale}
					selectedUnit={selectedUnit}
					isHovered={tooltipOpen}
				/>
				<AxisBottom
					top={Math.max(height - 30, 0)}
					orientation="bottom"
					tickLabelProps={{
						fontFamily: 'none',
						fontSize: 'none',
						stroke: 'none',
						fill: 'none',
						className: 'text-subtitleSmall font-medium fill-steel font-sans',
					}}
					scale={xScale}
					tickFormat={String}
					hideTicks
					hideAxisLine
					tickValues={xScale.ticks(totalTicks).filter(Number.isInteger)}
				/>
				<MarkerCircle id="marker-circle" className="fill-steel" size={1} refX={1} />
				<LinePath
					data={adjData}
					x={(d) => xScale(d.epoch)}
					y={Math.max(height - 5, 0)}
					stroke="transparent"
					markerStart="url(#marker-circle)"
					markerMid="url(#marker-circle)"
					markerEnd="url(#marker-circle)"
					className={clsx(
						'transition-all ease-ease-out-cubic',
						tooltipOpen ? 'opacity-100' : 'opacity-0',
					)}
				/>
				<rect
					x={0}
					y={graphTop}
					width={width}
					height={Math.max(height - graphTop, 0)}
					fill="transparent"
					stroke="none"
					onMouseEnter={(e) => {
						handleTooltipThrottled?.(localPoint(e)?.x || SIDE_MARGIN);
					}}
					onMouseMove={(e) => {
						handleTooltipThrottled?.(localPoint(e)?.x || SIDE_MARGIN);
					}}
					onMouseLeave={() => {
						handleTooltipThrottled?.cancel({
							upcomingOnly: true,
						});
						hideTooltip();
					}}
				/>
			</svg>
		</div>
	);
}
