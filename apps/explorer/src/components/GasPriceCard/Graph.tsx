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
    const graphTop = 0;
    const graphButton = Math.max(height - 20, 0);
    const xScale = useMemo(
        () =>
            scaleTime<number>({
                domain: extent(data, ({ date }) => date) as [Date, Date],
                range: [SIDE_MARGIN, width - SIDE_MARGIN],
            }),
        [width, data]
    );
    const yScale = useMemo(
        () =>
            scaleLinear<number>({
                domain: extent(data, ({ referenceGasPrice }) =>
                    referenceGasPrice !== null
                        ? Number(referenceGasPrice)
                        : null
                ) as [number, number],
                range: [graphTop, graphButton],
            }),
        [data, graphTop, graphButton]
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
            const epochIndex = bisectDate(data, xDate, 0);
            const selectedEpoch = data[epochIndex];
            setTooltipX(x);
            setHoveredElement(selectedEpoch);
        },
        [xScale, data]
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
                y1={graphTop + 5}
                x2={0}
                y2={graphButton - 5}
                className={clsx(
                    'stroke-gray-60 transition-all duration-75',
                    isTooltipVisible ? 'visible' : 'invisible'
                )}
                strokeWidth="1"
                transform={`translate(${tooltipX})`}
            />
            <LinePath<EpochGasInfo>
                curve={curveLinear}
                data={data}
                x={(d) => xScale(d.date!.getTime())}
                y={(d) => yScale(Number(d.referenceGasPrice!))}
                width="1"
                markerMid="url(#marker-circle)"
                markerStart="url(#marker-circle)"
                markerEnd="url(#marker-circle)"
                defined={isDefined}
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
                y={graphTop}
                width={Math.max(0, width - SIDE_MARGIN * 2)}
                height={graphButton - graphTop}
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
