// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { Fragment } from 'react';

import { Text, type TextProps } from '../Text';

export default {
	component: Text,
} as Meta;

interface StoryProps {
	variants: TextProps['variant'][];
	italic?: boolean;
}

export const Body: StoryObj<StoryProps> = {
	render: ({ variants, italic }) => (
		<div>
			{variants.map((variant) => (
				<Fragment key={variant}>
					<Text key={variant} variant={variant}>
						{variant}
					</Text>

					{italic && (
						<Text variant={variant} italic>
							{variant} - Italic
						</Text>
					)}
				</Fragment>
			))}
		</div>
	),
	args: {
		variants: ['body/medium', 'body/semibold', 'bodySmall/medium', 'bodySmall/semibold'],
		italic: true,
	},
};

export const Subtitle: StoryObj<StoryProps> = {
	...Body,
	args: {
		variants: [
			'subtitle/medium',
			'subtitle/semibold',
			'subtitleSmall/medium',
			'subtitleSmall/semibold',
			'subtitleSmallExtra/medium',
			'subtitleSmallExtra/semibold',
		],
	},
};

export const Caption: StoryObj<StoryProps> = {
	...Body,
	args: {
		variants: [
			'caption/medium',
			'caption/semibold',
			'caption/bold',
			'captionSmall/medium',
			'captionSmall/semibold',
			'captionSmall/bold',
		],
	},
};
