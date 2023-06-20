// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Pill } from './Pill';

export default {
	component: Pill,
} as Meta<typeof Pill>;

export const Default: StoryObj<typeof Pill> = {
	render: (props) => (
		<div className="flex flex-wrap gap-2">
			<Pill {...props} to="/" />
			<Pill {...props} href="https://example.com" />
			<Pill {...props} onClick={() => alert('Hello')} />
			<Pill {...props} to="/" disabled />
			<Pill {...props} href="https://example.com" disabled />
			<Pill {...props} onClick={() => alert('Hello')} disabled />
			<Pill {...props} to="/" loading />
			<Pill {...props} href="https://example.com" loading />
			<Pill {...props} onClick={() => alert('Hello')} loading />
		</div>
	),
	args: {
		text: 'Default Link',
	},
};
