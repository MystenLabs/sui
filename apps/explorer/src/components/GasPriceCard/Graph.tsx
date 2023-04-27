// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom } from '@visx/axis';
import { curveLinear } from '@visx/curve';
import { localPoint } from '@visx/event';
import { scaleTime, scaleLinear } from '@visx/scale';
import { LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';

import { type EpochGasInfo } from './types';

function formatDate(date: Date) {
    return ['S', 'M', 'T', 'W', 'T', 'F', 'S'][date.getDay()];
}

function isDefined(d: EpochGasInfo) {
    return d.date !== null && d.referenceGasPrice !== null;
}

const ONE_DAY_MILLIS = 1000 * 60 * 60 * 24;
const SIDE_MARGIN = 30;

const bisectDate = bisector(({ date }: EpochGasInfo) => date).center;

export type GraphProps = {
    data: EpochGasInfo[];
    width: number;
    height: number;
    onHoverElement: (value: EpochGasInfo | null) => void;
};
export function Graph({ data, width, height, onHoverElement }: GraphProps) {
    // remove not defined data (graph displays better and helps with hovering/selecting hovered element)
    const adjData = useMemo(() => data.filter(isDefined), [data]);
    const graphTop = 5;
    const graphButton = Math.max(height - 35, 0);
    const xScale = useMemo(
        () =>
            scaleTime<number>({
                domain: extent(adjData, ({ date }) => date) as [Date, Date],
                range: [SIDE_MARGIN, width - SIDE_MARGIN],
                nice: 'hour',
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
            const xDate = xScale.invert(x);
            const epochIndex = bisectDate(adjData, xDate, 0);
            const selectedEpoch = adjData[epochIndex];
            setTooltipX(x);
            setHoveredElement(selectedEpoch);
        },
        [xScale, adjData]
    );
    const handleTooltipThrottledRef = useRef(throttle(100, handleTooltip));
    useEffect(() => {
        handleTooltipThrottledRef.current = throttle(100, handleTooltip);
        return () => {
            handleTooltipThrottledRef?.current?.cancel?.();
        };
    }, [handleTooltip]);
    const handleTooltipThrottled = handleTooltipThrottledRef.current;
    const totalDays = useMemo(() => {
        const domain = xScale.domain();
        return Math.floor(
            (domain[1].getTime() - domain[0].getTime()) / ONE_DAY_MILLIS
        );
    }, [xScale]);
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
                    'stroke-gray-60 transition-all duration-75',
                    isTooltipVisible ? 'visible' : 'invisible'
                )}
                strokeWidth="1"
                transform={`translate(${tooltipX})`}
            />
            <LinePath<EpochGasInfo>
                curve={curveLinear}
                data={adjData}
                x={(d) => xScale(d.date!.getTime())}
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
                tickFormat={(date) => formatDate(date as Date)}
                hideTicks
                hideAxisLine
                numTicks={Math.min(totalDays || 0, 15)}
            />
            <rect
                x={SIDE_MARGIN}
                y={graphTop - 5}
                width={Math.max(0, width - SIDE_MARGIN * 2)}
                height={Math.max(0, graphButton - graphTop + 5)}
                fill="transparent"
                stroke="none"
                onMouseEnter={(e) => {
                    handleTooltipThrottled(localPoint(e)?.x || SIDE_MARGIN);
                    setIsTooltipVisible(true);
                }}
                onMouseMove={(e) => {
                    handleTooltipThrottled(localPoint(e)?.x || SIDE_MARGIN);
                }}
                onMouseLeave={() => {
                    handleTooltipThrottled.cancel({ upcomingOnly: true });
                    setIsTooltipVisible(false);
                    setHoveredElement(null);
                }}
            />
        </svg>
    );
}
