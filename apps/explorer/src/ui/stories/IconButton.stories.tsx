// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X12 } from '@mysten/icons';
import { type StoryObj, type Meta } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import { IconButton, type IconButtonProps } from '../IconButton';

export default {
	component: IconButton,
	decorators: [
		(Story) => (
			<MemoryRouter>
				<Story />
			</MemoryRouter>
		),
	],
} as Meta;

export const CloseIcon: StoryObj<IconButtonProps> = {
	render: (props) => (
		<div className="flex flex-col items-start gap-2">
			<IconButton href="/relative" {...props} />
			<IconButton {...props} />
			<IconButton href="https://google.com" {...props} />
			<IconButton onClick={() => alert('on click')} {...props} />
			<IconButton disabled {...props} />
		</div>
	),
	args: { icon: X12 },
};
