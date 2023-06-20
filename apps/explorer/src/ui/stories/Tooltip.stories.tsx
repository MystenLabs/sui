// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { IconTooltip, type IconTooltipProps } from '../Tooltip';

export default {
	component: IconTooltip,
} as Meta;

export const Tooltip: StoryObj<IconTooltipProps> = {
	render: (props) => <IconTooltip {...props} tip="Test text tooltip" />,
	args: {},
};

export const TooltipBottom: StoryObj<IconTooltipProps> = {
	...Tooltip,
	args: {
		placement: 'bottom',
	},
};

export const TooltipLeft: StoryObj<IconTooltipProps> = {
	...Tooltip,
	args: {
		placement: 'left',
	},
};

export const TooltipRight: StoryObj<IconTooltipProps> = {
	...Tooltip,
	args: {
		placement: 'right',
	},
};
