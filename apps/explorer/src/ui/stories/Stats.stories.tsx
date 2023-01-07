// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Stats, type StatsProps } from '../Stats';

export default {
    component: Stats,
} as Meta;

export const defaultAmount: StoryObj<StatsProps> = {
    args: {
        label: 'Last Epoch Change',
        value: '8,109',
        tooltip: 'Last Epoch Change Tooltip',
    },
};
