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
    },
    render: (args) => (
        <div className="flex w-1/2">
            <ProgressBar {...args} />
        </div>
    ),
};

export const Animated: StoryObj<ProgressBarProps> = {
    ...Default,
    args: {
        progress: 75,
        animate: true,
    },
};
