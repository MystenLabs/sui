// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom } from '@visx/axis';
import { curveNatural } from '@visx/curve';
import { localPoint } from '@visx/event';
import { scaleLinear } from '@visx/scale';
import { LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';

import { type EpochGasInfo } from './types';

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
    onHoverElement: (value: EpochGasInfo | null) => void;
};
export function Graph({ data, width, height, onHoverElement }: GraphProps) {
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
        [adjData, graphTop, graphButton]
    );
    const [isTooltipVisible, setIsTooltipVisible] = useState(false);
    const [tooltipX, setTooltipX] = useState(SIDE_MARGIN);
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
    const totalTicks = Math.min(
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
                y1={graphTop - 5}
                x2={0}
                y2={graphButton + 10}
                className={clsx(
                    'stroke-gray-60 transition-all ease-ease-out-cubic',
                    isTooltipVisible ? 'opacity-100' : 'opacity-0'
                )}
                strokeWidth="1"
                transform={`translate(${tooltipX})`}
            />
            <LinePath<EpochGasInfo>
                curve={curveNatural}
                data={adjData}
                x={(d) => xScale(d.epoch)}
                y={(d) => yScale(Number(d.referenceGasPrice!))}
                width="1"
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
                numTicks={totalTicks}
            />
            <rect
                x={SIDE_MARGIN}
                y={graphTop - 5}
                width={Math.max(0, width - SIDE_MARGIN * 2)}
                height={Math.max(0, graphButton - graphTop + 5)}
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
