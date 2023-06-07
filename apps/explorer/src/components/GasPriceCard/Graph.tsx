// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AxisBottom } from '@visx/axis';
import { curveLinear } from '@visx/curve';
import { localPoint } from '@visx/event';
import { scaleLinear } from '@visx/scale';
import { AreaClosed, LinePath } from '@visx/shape';
import clsx from 'clsx';
import { bisector, extent } from 'd3-array';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { throttle } from 'throttle-debounce';
import { MarkerCircle } from '@visx/marker';

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
    const graphTop = 0;
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
    const yScale = useMemo(() => {
        const [min, max] = extent(adjData, ({ referenceGasPrice }) =>
            referenceGasPrice !== null ? Number(referenceGasPrice) : null
        );
        console.log([(min || 0) * 0.95, (max || 0) * 1.05]);
        return scaleLinear<number>({
            domain: [(min || 0) * 0.75, (max || 0) * 1.05],
            range: [graphButton, graphTop],
        });
    }, [adjData, graphTop, graphButton]);
    const [isTooltipVisible, setIsTooltipVisible] = useState(false);
    const [tooltipX, setTooltipX] = useState(SIDE_MARGIN);
    const [tooltipY, setTooltipY] = useState(height);
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
                y2={height - 10}
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
            <MarkerCircle id="marker-circle" fill="#333" size={0.5} refX={1} />
            <LinePath
                data={adjData}
                x={(d) => xScale(d.epoch)}
                y={height - 5}
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
