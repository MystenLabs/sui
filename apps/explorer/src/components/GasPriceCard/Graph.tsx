// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom, AxisRight } from '@visx/axis';
import { curveMonotoneX } from '@visx/curve';
import { localPoint } from '@visx/event';
import { scaleLinear } from '@visx/scale';
import { AreaClosed, LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';

import { type EpochGasInfo } from './types';
import { UnitsType } from '.';

function formatXLabel(epoch: number) {
    return String(epoch);
}

export function isDefined(d: EpochGasInfo) {
    return d.date !== null && d.referenceGasPrice !== null;
}

const SIDE_MARGIN = 30;

const bisectEpoch = bisector(({ epoch }: EpochGasInfo) => epoch).center;


export type GraphProps = {
    data: EpochGasInfo[];
    width: number;
    height: number;
    unit: UnitsType
    onHoverElement: (value: EpochGasInfo | null) => void;
};
export function Graph({ data, width, height, onHoverElement, unit }: GraphProps) {
    // remove not defined data (graph displays better and helps with hovering/selecting hovered element)
    const adjData = useMemo(() => data.filter(isDefined), [data]);
    const graphTop = 10;
    const graphTooltipOffset = Math.max(height - 45, 0);
    const yMax = Math.ceil(
        Math.max(
            ...adjData.map(({ referenceGasPrice }) =>
                referenceGasPrice !== null ? Number(referenceGasPrice) : 0
            )
        ) / 25
    ) * 25 + 25;

    const yMin = Math.floor(
        Math.min(
            ...adjData.map(({ referenceGasPrice }) =>
                referenceGasPrice !== null ? Number(referenceGasPrice) : Number.MAX_VALUE
            )
        ) / 25
    ) * 25;
    const xScaleArea = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(adjData, ({ epoch }) => epoch) as [
                    number,
                    number
                ],
                range: [0, width],
            }),
        [width, adjData]
    );
    const xScaleGraph = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(adjData, ({ epoch }) => epoch) as [
                    number,
                    number
                ],
                range: [10, width],
            }),
        [width, adjData]
    );
    const yScaleLine = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(adjData, ({ referenceGasPrice }) =>
                    referenceGasPrice !== null
                        ? Number(referenceGasPrice)
                        : null
                ) as [number, number],
                range: [height - 40, 30],
            }),
        [adjData, graphTop, graphTooltipOffset]
    );
    const yScaleArea = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(adjData, ({ referenceGasPrice }) =>
                    referenceGasPrice !== null
                        ? Number(referenceGasPrice)
                        : null
                ) as [number, number],
                range: [height, graphTop],
            }),
        [adjData, graphTop, graphTooltipOffset]
    );
    const [isTooltipVisible, setIsTooltipVisible] = useState(false);
    const [tooltipX, setTooltipX] = useState(SIDE_MARGIN);
    const [tooltipY, setTooltipY] = useState(0);
    const [hoveredElement, setHoveredElement] = useState<EpochGasInfo | null>(
        null
    );
    useEffect(() => {
        onHoverElement(hoveredElement);
    }, [onHoverElement, hoveredElement]);
    const handleTooltip = useCallback(
        (x: number) => {
            const xEpoch = xScaleGraph.invert(x);
            const epochIndex = bisectEpoch(adjData, xEpoch, 0);
            const selectedEpoch = adjData[epochIndex];
            setTooltipX(xScaleGraph(selectedEpoch.epoch));
            setTooltipY(yScaleLine(Number(selectedEpoch.referenceGasPrice)));
            setHoveredElement(selectedEpoch);
            setIsTooltipVisible(true);
        },
        [xScaleGraph, adjData, yScaleLine]
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
    const totalHorizontalTicks = Math.min(
        adjData.length,
        totalMaxTicksForWidth < 1 ? 1 : totalMaxTicksForWidth
    );

    const middleYValue = (yMax + yMin) / 2;
    const yTicks = [yMax, middleYValue, yMin];
    const xTicks = adjData.filter((_, index) => index % 5 === 0 || index === 0).map(d => d.epoch);
    return (
        <svg
            width={width}
            height={height}
            className="stroke-steel-dark/80 transition hover:stroke-hero"
        >
            <line
                x1={0}
                y1={graphTop - 10}
                x2={0}
                y2={height}
                className={clsx(
                    'stroke-gray-60 transition-all ease-ease-out-cubic',
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
                    'stroke-gray-60 transition-all ease-ease-out-cubic',
                    isTooltipVisible ? 'opacity-100' : 'opacity-0'
                )}
                strokeWidth="1"
                transform={`translate(0 ${tooltipY})`}
            />
            <AreaClosed<EpochGasInfo>
                data={adjData}
                yScale={yScaleArea}
                x={(d) => xScaleArea(d.epoch)}
                y={(d) => yScaleLine(Number(d.referenceGasPrice!))}
                strokeWidth={0}
                stroke="#F2BD24"
                curve={curveMonotoneX}
                fill="#F2BD24"
                fillOpacity={0.1}
                strokeOpacity={1}
            />
            <LinePath<EpochGasInfo>
                curve={curveMonotoneX}
                data={adjData}
                x={(d) => xScaleArea(d.epoch)}
                y={(d) => yScaleLine(Number(d.referenceGasPrice!))}
                width="1"
                stroke="#F2BD24"
            />
            <AxisBottom
                top={height - 30}
                orientation="bottom"
                tickLabelProps={{
                    fontFamily: 'none',
                    fontSize: 'none',
                    stroke: 'none',
                    fill: 'none',
                    className: 'text-subtitle font-medium fill-steel font-sans',
                }}
                hideTicks
                tickValues={xTicks}
                scale={xScaleGraph}
                tickFormat={(epoch) => formatXLabel(epoch as number)}
                hideAxisLine
                numTicks={totalHorizontalTicks}
                left={0}
            />
            <AxisRight
                tickLabelProps={{
                    fontFamily: 'none',
                    fontSize: 'none',
                    stroke: 'none',
                    fill: 'none',
                    className: 'text-subtitle font-medium fill-steel font-sans',
                }}
                left={width - 30}
                scale={yScaleLine}
                hideTicks
                hideAxisLine
                tickValues={yTicks}
                orientation="left"
            />
            <rect
                x={0}
                y={graphTop - 10}
                width={width}
                height={height}
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
