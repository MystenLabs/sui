// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { IconTooltip } from '../Tooltip';

import type { UseTooltipStateProps } from '../Tooltip';

export default {
    component: IconTooltip,
} as Meta;

type StoryProps = UseTooltipStateProps;

export const Tooltip: StoryObj<StoryProps> = {
    render: (props) => <IconTooltip {...props}>Test text tooltip</IconTooltip>,
    args: {},
};

export const TooltipBottom: StoryObj<StoryProps> = {
    ...Tooltip,
    args: {
        placement: 'bottom',
    },
};

export const TooltipLeft: StoryObj<StoryProps> = {
    ...Tooltip,
    args: {
        placement: 'left',
    },
};

export const TooltipRight: StoryObj<StoryProps> = {
    ...Tooltip,
    args: {
        placement: 'right',
    },
};
