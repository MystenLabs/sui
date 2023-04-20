// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';

import { type Gradient, type RingChartData } from './RingChart';

import { Heading } from '~/ui/Heading';

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
