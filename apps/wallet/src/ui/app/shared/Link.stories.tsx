// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight12 } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';

import { Link } from './Link';

export default {
	component: Link,
} as Meta<typeof Link>;

export const Default: StoryObj<typeof Link> = {
	render: (props) => (
		<>
			<Link {...props} to="/" />
			<Link {...props} href="https://example.com" />
			<Link {...props} onClick={() => alert('Hello')} />
			<Link {...props} to="/" disabled />
			<Link {...props} href="https://example.com" disabled />
			<Link {...props} onClick={() => alert('Hello')} disabled />
			<Link {...props} to="/" loading />
			<Link {...props} href="https://example.com" loading />
			<Link {...props} onClick={() => alert('Hello')} loading />
		</>
	),
	args: {
		text: 'Default Link',
		after: <ArrowUpRight12 />,
		color: 'steelDark',
		weight: 'semibold',
	},
};
