// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { Fragment } from 'react';

import { Heading } from '~/ui/Heading';

export type Gradient = {
    deg?: number;
    values: { percent: number; color: string }[];
};

type RingChartData = {
    value: number;
    label: string;
    color?: string;
    gradient?: Gradient;
}[];

interface RingChartLegendProps {
    data: RingChartData;
    title: string;
}

function getColorFromGradient({ deg, values }: Gradient) {
    const gradientResult = [];

    if (deg) {
        gradientResult.push(`${deg}deg`);
    }

    const valuesMap = values.map(
        ({ percent, color }) => `${color} ${percent}%`
    );

    gradientResult.push(...valuesMap);

    return `linear-gradient(${gradientResult.join(',')})`;
}

export function RingChartLegend({ data, title }: RingChartLegendProps) {
    return (
        <div className="flex flex-col gap-2">
            <Heading variant="heading5/semibold" color="steel-darker">
                {title}
            </Heading>

            <div className="flex flex-col items-start justify-center gap-2">
                {data.map(({ color, gradient, label, value }) => {
                    const colorDisplay = gradient
                        ? getColorFromGradient(gradient)
                        : color;

                    return (
                        <div
                            className={clsx(
                                'flex items-center gap-1.5',
                                value === 0 && 'hidden'
                            )}
                            key={label}
                        >
                            <div
                                style={{ background: colorDisplay }}
                                className="h-3 w-3 rounded-sm"
                            />
                            <div
                                style={{
                                    backgroundImage: colorDisplay,
                                    color: colorDisplay,
                                }}
                                className="bg-clip-text text-body font-medium text-transparent"
                            >
                                {value} {label}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

export interface RingChartProps {
    data: RingChartData;
}

export function RingChart({ data }: RingChartProps) {
    const radius = 20;
    const cx = 25;
    const cy = 25;
    const dashArray = 2 * Math.PI * radius;
    const startAngle = -90;
    const total = data.reduce((acc, { value }) => acc + value, 0);
    let filled = 0;

    const segments = data.map(({ value, label, color, gradient }, idx) => {
        const gradientId = `gradient-${idx}`;
        const ratio = (100 / total) * value;
        const angle = (filled * 360) / 100 + startAngle;
        const offset = dashArray - (dashArray * ratio) / 100;
        filled += ratio;
        return (
            <Fragment key={label}>
                {gradient && (
                    <defs>
                        <linearGradient id={gradientId}>
                            {gradient.values.map(({ percent, color }, i) => (
                                <stop
                                    key={i}
                                    offset={percent}
                                    stopColor={color}
                                />
                            ))}
                        </linearGradient>
                    </defs>
                )}
                <circle
                    cx={cx}
                    cy={cy}
                    r={radius}
                    fill="transparent"
                    stroke={gradient ? `url(#${gradientId})` : color}
                    strokeWidth={5}
                    strokeDasharray={dashArray}
                    strokeDashoffset={offset}
                    transform={`rotate(${angle} ${cx} ${cy})`}
                />
            </Fragment>
        );
    });

    return (
        <div className="relative">
            <svg viewBox="0 0 50 50" strokeLinecap="butt">
                {segments}
            </svg>
            <div className="absolute inset-0 mx-auto flex items-center justify-center">
                <div className="flex flex-col items-center gap-1.5">
                    <Heading variant="heading2/semibold" color="sui-dark">
                        {total}
                    </Heading>
                </div>
            </div>
        </div>
    );
}
