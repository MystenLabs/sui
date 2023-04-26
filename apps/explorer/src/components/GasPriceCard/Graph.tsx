// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom } from '@visx/axis';
import { curveLinear } from '@visx/curve';
import { localPoint } from '@visx/event';
import { scaleTime, scaleLinear } from '@visx/scale';
import { LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector } from 'd3-array';
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
    durationDays: number;
    onHoverElement: (value: EpochGasInfo | null) => void;
};
export function Graph({
    data,
    width,
    height,
    durationDays,
    onHoverElement,
}: GraphProps) {
    const graphTop = 0;
    const graphButton = Math.max(height - 20, 0);
    const xScale = useMemo(() => {
        const today = new Date();
        today.setHours(0);
        today.setMinutes(0);
        today.setSeconds(0);
        today.setMilliseconds(0);
        const minDate = new Date(today);
        minDate.setTime(
            minDate.getTime() - (durationDays - 1) * ONE_DAY_MILLIS
        );
        const minGraphDate = Math.min(
            ...data.filter(isDefined).map(({ date }) => date!.getTime())
        );
        return scaleTime<number>({
            domain: [
                new Date(Math.min(minDate.getTime(), minGraphDate)),
                today,
            ],
            nice: 'day',
            range: [SIDE_MARGIN, width - SIDE_MARGIN],
        });
    }, [durationDays, width, data]);
    const yScale = useMemo(() => {
        const prices = [
            ...data
                .filter(isDefined)
                .map(({ referenceGasPrice }) => Number(referenceGasPrice!)),
        ];
        return scaleLinear<number>({
            domain: [Math.min(...prices), Math.max(...prices)],
            range: [graphTop, graphButton],
        });
    }, [data, graphTop, graphButton]);
    const [isTooltipVisible, setIsTooltipVisible] = useState(false);
    const [tooltipX, setTooltipX] = useState(SIDE_MARGIN);
    const [hoveredElement, setHoveredElement] = useState<EpochGasInfo | null>(
        null
    );
    useEffect(() => {
        onHoverElement(hoveredElement);
    }, [onHoverElement, hoveredElement]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
    const handleTooltipThrottled = useCallback(
        throttle(100, (x: number) => {
            const xDate = xScale.invert(x);
            const epochIndex = bisectDate(data, xDate, 0);
            const selectedEpoch = data[epochIndex];
            setTooltipX(x);
            setHoveredElement(selectedEpoch);
        }),
        [xScale, data]
    );
    const prevHandleTooltipThrottled = useRef<typeof handleTooltipThrottled>();
    useEffect(() => {
        console.log('set prevHandleTooltipThrottled', handleTooltipThrottled);
        prevHandleTooltipThrottled.current = handleTooltipThrottled;
        return () => {
            console.log(
                'clear prevHandleTooltipThrottled',
                prevHandleTooltipThrottled?.current
            );
            prevHandleTooltipThrottled?.current?.cancel?.({
                upcomingOnly: true,
            });
        };
    }, [handleTooltipThrottled]);
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
                tickFormat={(data) => formatDate(data as Date)}
                hideTicks
                hideAxisLine
                numTicks={Math.min(durationDays, 15)}
            />
            <rect
                x={SIDE_MARGIN}
                y={graphTop}
                width={Math.max(0, width - SIDE_MARGIN * 2)}
                height={graphButton - graphTop}
                fill="transparent"
                stroke="none"
                onMouseEnter={() => {
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
