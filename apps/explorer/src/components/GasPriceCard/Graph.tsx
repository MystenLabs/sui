// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom, AxisRight } from '@visx/axis';
import { curveNatural } from '@visx/curve';
import { localPoint } from '@visx/event';
import { scaleLinear } from '@visx/scale';
import { LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';

import { type EpochGasInfo } from './types';
import { CoinFormat, formatBalance } from '@mysten/core';
import { SUI_DECIMALS } from '@mysten/sui.js';

function formatXLabel(epoch: number) {
    return String(epoch);
}

const UNITS = ['MIST', 'SUI'] as const;
type UnitsType = (typeof UNITS)[number];

export function isDefined(d: EpochGasInfo) {
    return d.date !== null && d.referenceGasPrice !== null;
}

const SIDE_MARGIN = 35;

const bisectEpoch = bisector(({ epoch }: EpochGasInfo) => epoch).center;

function useGasPriceFormat(gasPrice: bigint | number, unit: 'MIST' | 'SUI') {
    return formatBalance(
        gasPrice,
        unit === 'MIST' ? 0 : SUI_DECIMALS,
        CoinFormat.FULL
    )
}

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
    const graphTop = 15;
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
                ) as [number, number],
                range: [graphButton, graphTop],
            }),
        [adjData, graphTop, graphButton, unit]
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
            const xEpoch = xScale.invert(x);
            const epochIndex = bisectEpoch(adjData, xEpoch, 0);
            const selectedEpoch = adjData[epochIndex];
            setTooltipX(xScale(selectedEpoch.epoch));
            setTooltipY(yScale(Number(selectedEpoch.referenceGasPrice)));
            setHoveredElement(selectedEpoch);
            setIsTooltipVisible(true);
        },
        [xScale, adjData]
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
    const totalMaxTicksForLength = Math.floor((height - 2 * SIDE_MARGIN) / 28);
    const totalVerticalTicks = Math.min(
        adjData.length,
        totalMaxTicksForLength < 1 ? 1 : totalMaxTicksForLength
    );
    const totalHorizontalTicks = Math.min(
        adjData.length,
        totalMaxTicksForWidth < 1 ? 1 : totalMaxTicksForWidth
    );
    return (
        <svg
            width={width}
            height={height}
            className="stroke-steel-dark/80 transition hover:stroke-hero"
        >
            <line
                x1={0}
                y1={graphTop - 40}
                x2={0}
                y2={graphButton + 10}
                className={clsx(
                    'stroke-gray-60 transition-all ease-ease-out-cubic',
                    isTooltipVisible ? 'opacity-100' : 'opacity-0'
                )}
                strokeWidth="1"
                transform={`translate(${tooltipX})`}
            />
            <line
                x1={SIDE_MARGIN}
                y1={0}
                x2={width - SIDE_MARGIN}
                y2={0}
                className={clsx(
                    'stroke-gray-60 transition-all ease-ease-out-cubic',
                    isTooltipVisible ? 'opacity-100' : 'opacity-0'
                )}
                strokeWidth="1"
                transform={`translate(0 ${tooltipY})`}
            />
            <LinePath<EpochGasInfo>
                curve={curveNatural}
                data={adjData}
                x={(d) => xScale(d.epoch)}
                y={(d) => yScale(Number(d.referenceGasPrice!))}
                width="1"
                stroke='#F2BD24'
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
                scale={xScale}
                tickFormat={(epoch) => formatXLabel(epoch as number)}
                hideTicks
                hideAxisLine
                numTicks={totalHorizontalTicks}
            />
            <AxisRight
                tickLabelProps={{
                    fontFamily: 'none',
                    fontSize: 'none',
                    stroke: 'none',
                    fill: 'none',
                    className: 'text-subtitle font-medium fill-steel font-sans',
                }}
                left={5 - SIDE_MARGIN / 2}
                scale={yScale}
                tickFormat={(price) => useGasPriceFormat(Number(price), unit)}
                hideTicks
                hideAxisLine
                numTicks={totalVerticalTicks}
            />
            <rect
                x={SIDE_MARGIN}
                y={graphTop - 10}
                width={Math.max(0, width - SIDE_MARGIN * 2)}
                height={Math.max(0, graphButton - graphTop + 10)}
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
