// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate } from '@mysten/core';
import { AxisBottom } from '@visx/axis';
import { curveLinear } from '@visx/curve';
import { localPoint } from '@visx/event';
import { MarkerCircle } from '@visx/marker';
import { ParentSize } from '@visx/responsive';
import { scaleLinear } from '@visx/scale';
import { AreaClosed, LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';

import { ErrorBoundary } from '../error-boundary/ErrorBoundary';
import { type EpochGasInfo } from './types';
import { type UnitsType, useGasPriceFormat } from './utils';

import { Text } from '~/ui/Text';

function formatXLabel(epoch: number) {
    return String(epoch);
}

export function isDefined(d: EpochGasInfo) {
    return d.date !== null && d.referenceGasPrice !== null;
}

const SIDE_MARGIN = 15;

const bisectEpoch = bisector(({ epoch }: EpochGasInfo) => epoch).center;

type AxisRightProps<Output> = {
    left: number;
    scale: ReturnType<typeof scaleLinear<Output>>;
};

function AxisRight({ scale, left }: AxisRightProps<number>) {
    let ticks = scale.nice(6).ticks(6);
    return (
        <g>
            <g>
                {ticks
                    .filter(
                        (_, index) =>
                            (index + 1) % 2 === 0 || ticks.length === 1
                    )
                    .map((value) => (
                        <text
                            key={value}
                            x={left}
                            y={scale(value)}
                            textAnchor="end"
                            alignmentBaseline="middle"
                            className="fill-steel font-sans text-subtitleSmall font-medium"
                        >
                            {value}
                        </text>
                    ))}
            </g>
            <g>
                {ticks
                    .filter(
                        (_, index) =>
                            (index + 1) % 2 !== 0 && ticks.length !== 1
                    )
                    .map((value) => (
                        <circle
                            key={value}
                            cx={left}
                            cy={scale(value)}
                            r="1"
                            className="fill-steel"
                        />
                    ))}
            </g>
        </g>
    );
}

export type GraphWithTooltipProps = {
    data: EpochGasInfo[];
    unit: UnitsType;
};
export function GraphWithTooltip({ data, unit }: GraphWithTooltipProps) {
    const [hoveredElement, setHoveredElement] = useState<EpochGasInfo | null>(
        null
    );
    const [tooltipX, setTooltipX] = useState(0);
    const formattedHoveredPrice = useGasPriceFormat(
        hoveredElement?.referenceGasPrice ?? null,
        unit
    );
    const formattedHoveredDate = hoveredElement?.date
        ? formatDate(hoveredElement?.date, ['month', 'day'])
        : '-';
    return (
        <div className="relative flex flex-1 self-stretch">
            <div
                className={clsx(
                    'absolute top-0 z-10 flex -translate-x-[1px] -translate-y-[calc(100%-10px)] flex-col flex-nowrap gap-0.5 whitespace-nowrap rounded-md rounded-bl-none border border-solid border-gray-45 bg-gray-90 px-2 py-1.5',
                    hoveredElement?.date ? 'visible' : 'invisible'
                )}
                style={{ left: tooltipX }}
            >
                <Text variant="caption/semibold" color="white">
                    <div className="whitespace-nowrap">
                        {formattedHoveredPrice
                            ? `${formattedHoveredPrice} ${unit}`
                            : '-'}
                    </div>
                </Text>
                <Text variant="subtitleSmallExtra/medium" color="white">
                    Epoch {hoveredElement?.epoch}, {formattedHoveredDate}
                </Text>
            </div>
            <div className="relative flex-1 self-stretch">
                <ErrorBoundary>
                    <ParentSize className="absolute">
                        {(parent) => (
                            <Graph
                                width={parent.width}
                                height={parent.height}
                                data={data}
                                onHoverElement={(data) => {
                                    setHoveredElement(data?.value || null);
                                    setTooltipX(data?.x || 0);
                                }}
                            />
                        )}
                    </ParentSize>
                </ErrorBoundary>
            </div>
        </div>
    );
}

export type GraphProps = {
    data: EpochGasInfo[];
    width: number;
    height: number;
    onHoverElement: (params: { value: EpochGasInfo; x: number } | null) => void;
};
export function Graph({ data, width, height, onHoverElement }: GraphProps) {
    // remove not defined data (graph displays better and helps with hovering/selecting hovered element)
    const adjData = useMemo(() => data.filter(isDefined), [data]);
    const graphTop = 10;
    const graphButton = Math.max(height - 45, 0);
    const xScale = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(adjData, ({ epoch }) => epoch) as [
                    number,
                    number
                ],
                range: [SIDE_MARGIN, width - SIDE_MARGIN],
            }),
        [width, adjData]
    );
    const yScale = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(adjData, ({ referenceGasPrice }) =>
                    referenceGasPrice !== null
                        ? Number(referenceGasPrice)
                        : null
                ) as number[],
                range: [graphButton, graphTop],
            }),
        [adjData, graphTop, graphButton]
    );
    const [isTooltipVisible, setIsTooltipVisible] = useState(false);
    const [tooltipX, setTooltipX] = useState(SIDE_MARGIN);
    const [tooltipY, setTooltipY] = useState(height);
    const [hoveredElement, setHoveredElement] = useState<EpochGasInfo | null>(
        null
    );
    useEffect(() => {
        onHoverElement(
            hoveredElement ? { value: hoveredElement, x: tooltipX } : null
        );
    }, [onHoverElement, hoveredElement, tooltipX]);
    const handleTooltip = useCallback(
        (x: number) => {
            const xEpoch = xScale.invert(x);
            const epochIndex = bisectEpoch(adjData, xEpoch, 0);
            const selectedEpoch = adjData[epochIndex];
            setTooltipX(xScale(selectedEpoch.epoch));
            setTooltipY(yScale(Number(selectedEpoch.referenceGasPrice)));
            setHoveredElement(selectedEpoch);
            setIsTooltipVisible(true);
        },
        [xScale, adjData, yScale]
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
        totalMaxTicksForWidth < 1 ? 1 : totalMaxTicksForWidth
    );
    const firstElementY = adjData.length
        ? yScale(Number(adjData[0].referenceGasPrice))
        : null;
    const lastElementX = adjData.length
        ? xScale(adjData[adjData.length - 1].epoch)
        : null;
    const lastElementY = adjData.length
        ? yScale(Number(adjData[adjData.length - 1].referenceGasPrice))
        : null;
    if (height < 60 || width < 100) {
        return null;
    }
    return (
        <svg width={width} height={height}>
            <line
                x1={0}
                y1={Math.max(graphTop - 5, 0)}
                x2={0}
                y2={Math.max(height - 20, 0)}
                className={clsx(
                    'stroke-steel/30 transition-all ease-ease-out-cubic',
                    isTooltipVisible ? 'opacity-100' : 'opacity-0'
                )}
                strokeWidth="1"
                transform={`translate(${tooltipX})`}
            />
            <line
                x1={0}
                y1={0}
                x2={width}
                y2={0}
                className={clsx(
                    'stroke-steel/30 transition-all ease-ease-out-cubic',
                    isTooltipVisible ? 'opacity-100' : 'opacity-0'
                )}
                strokeWidth="1"
                transform={`translate(0, ${tooltipY})`}
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
                    <line
                        x1="0"
                        y1={firstElementY}
                        x2={SIDE_MARGIN}
                        y2={firstElementY}
                        stroke="#F2BD24"
                    />
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
            <AxisRight left={width - SIDE_MARGIN / 2} scale={yScale} />
            <AxisBottom
                top={Math.max(height - 30, 0)}
                orientation="bottom"
                tickLabelProps={{
                    fontFamily: 'none',
                    fontSize: 'none',
                    stroke: 'none',
                    fill: 'none',
                    className:
                        'text-subtitleSmall font-medium fill-steel font-sans',
                }}
                scale={xScale}
                tickFormat={(epoch) => formatXLabel(epoch as number)}
                hideTicks
                hideAxisLine
                tickValues={xScale.ticks(totalTicks).filter(Number.isInteger)}
            />
            <MarkerCircle
                id="marker-circle"
                className="fill-steel"
                size={1}
                refX={1}
            />
            <LinePath
                data={adjData}
                x={(d) => xScale(d.epoch)}
                y={Math.max(height - 5, 0)}
                stroke="transparent"
                markerStart="url(#marker-circle)"
                markerMid="url(#marker-circle)"
                markerEnd="url(#marker-circle)"
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
                    handleTooltipThrottled?.cancel({ upcomingOnly: true });
                    setIsTooltipVisible(false);
                    setHoveredElement(null);
                }}
            />
        </svg>
    );
}
