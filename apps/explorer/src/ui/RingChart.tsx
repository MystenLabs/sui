// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from './Heading';

export interface RingChartProps {
    data: {
        value: number;
        label: string;
        color: string;
    }[];
    radius?: number;
    suffix?: string;
    title?: string;
}

function Legend({ data, title }: Pick<RingChartProps, 'data' | 'title'>) {
    return (
        <div className="flex flex-col gap-2">
            <Heading variant="heading5/semibold" color="steel-darker">
                {title}
            </Heading>
            <div className="flex flex-col items-start justify-center gap-2">
                {data.map(({ color, label, value }) => (
                    <div className="flex items-center gap-1.5" key={label}>
                        <div
                            style={{ backgroundColor: color }}
                            className="h-3 w-3 rounded-sm"
                        />
                        <div
                            style={{ color: color }}
                            className="text-body font-medium"
                        >
                            {value} {label}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}

export function RingChart({ data, radius = 20, title }: RingChartProps) {
    const cx = 25;
    const cy = 25;
    const dashArray = 2 * Math.PI * radius;
    const startAngle = -90;
    const total = data.reduce((acc, { value }) => acc + value, 0);
    let filled = 0;
    const segments = data.map(({ value, label, color }) => {
        const ratio = (100 / total) * value;
        const angle = (filled * 360) / 100 + startAngle;
        const offset = dashArray - (dashArray * ratio) / 100;
        filled += ratio;
        return (
            <circle
                key={label}
                cx={cx}
                cy={cy}
                r={radius}
                fill="transparent"
                stroke={color}
                strokeWidth={5}
                strokeDasharray={dashArray}
                strokeDashoffset={offset}
                transform={`rotate(${angle} ${cx} ${cy})`}
            />
        );
    });

    return (
        <div className="flex items-center gap-5">
            <div className="relative h-24 min-h-[96px] w-24 min-w-[96px]">
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

            <div className="self-start">
                <Legend data={data} title={title} />
            </div>
        </div>
    );
}
