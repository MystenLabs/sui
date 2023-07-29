// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Heading, type HeadingProps } from './Heading';

const meta = {
	component: Heading,
} satisfies Meta<typeof Heading>;

export default meta;

type Story = StoryObj<{
	as: HeadingProps['as'];
	variants: HeadingProps['variant'][];
}>;

export const Heading1: Story = {
	render: ({ as, variants }) => (
		<div className="space-y-2">
			<div>
				{variants.map((variant) => (
					<Heading key={variant} as={as} variant={variant}>
						This is a sample heading.
					</Heading>
				))}
			</div>
			<div>
				{variants.map((variant) => (
					<Heading key={variant} as={as} variant={variant} fixed>
						This is a sample heading. (fixed)
					</Heading>
				))}
			</div>
		</div>
	),
	args: {
		as: 'h1',
		variants: ['heading1/bold', 'heading1/semibold', 'heading1/medium'],
	},
};

export const Heading2: Story = {
	...Heading1,
	args: {
		as: 'h2',
		variants: ['heading2/bold', 'heading2/semibold', 'heading2/medium'],
	},
};

export const Heading3: Story = {
	...Heading1,
	args: {
		as: 'h3',
		variants: ['heading3/bold', 'heading3/semibold', 'heading3/medium'],
	},
};

export const Heading4: Story = {
	...Heading1,
	args: {
		as: 'h4',
		variants: ['heading4/bold', 'heading4/semibold', 'heading4/medium'],
	},
};

export const Heading5: Story = {
	...Heading1,
	args: {
		as: 'h5',
		variants: ['heading5/bold', 'heading5/semibold', 'heading5/medium'],
	},
};

export const Heading6: Story = {
	...Heading1,
	args: {
		as: 'h6',
		variants: ['heading6/bold', 'heading6/semibold', 'heading6/medium'],
	},
};
