// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { ProgressBar, type ProgressBarProps } from '../ProgressBar';

export default {
    component: ProgressBar,
    parameters: {
        backgrounds: {
            default: 'gray100',
            values: [{ name: 'gray100', value: '#182435' }],
        },
    },
} as Meta;

export const Default: StoryObj<ProgressBarProps> = {
    args: {
        progress: 75,
    },
    render: (args) => (
        <div className="flex w-1/2">
            <ProgressBar {...args} />
        </div>
    ),
};

export const Animated: StoryObj<ProgressBarProps> = {
    render: () => {
        const progress = [5, 10, 25, 50, 75, 100];

        return (
            <div className="flex w-1/2 flex-col gap-4">
                {progress.map((p, index) => (
                    <ProgressBar key={p} progress={p} animate />
                ))}
            </div>
        );
    },
};
