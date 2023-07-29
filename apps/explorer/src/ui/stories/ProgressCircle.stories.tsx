// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';
import { type Meta, type StoryObj } from '@storybook/react';

import { ProgressCircle, type ProgressCircleProps } from '../ProgressCircle';

export default {
	component: ProgressCircle,
} as Meta;

export const Default: StoryObj<ProgressCircleProps> = {
	args: {
		progress: 50,
	},
	render: (args) => (
		<div className="justify flex items-center text-steel-darker">
			<div className="w-4">
				<ProgressCircle {...args} />
			</div>
			<Text variant="bodySmall/medium">50%</Text>
		</div>
	),
};
