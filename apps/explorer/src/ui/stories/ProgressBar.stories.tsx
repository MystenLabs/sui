// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { ProgressBar, type ProgressBarProps } from '../ProgressBar';

export default {
    component: ProgressBar,
} as Meta;

export const Default: StoryObj<ProgressBarProps> = {
    args: {
        progress: 75,
        repeatDelay: 0.1,
        ease: 'linear',
        movingDistance: 95,
    },
    render: (args) => (
        <div className="flex w-1/5">
            <ProgressBar {...args} />
        </div>
    ),
};
