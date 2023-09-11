// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClientProvider } from '@mysten/dapp-kit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';

import { ObjectVideoImage, type ObjectVideoImageProps } from '../ObjectVideoImage';

import type { Meta, StoryObj } from '@storybook/react';

export default {
	component: ObjectVideoImage,
	decorators: [
		(Story) => (
			<MemoryRouter>
				<QueryClientProvider client={new QueryClient()}>
					<SuiClientProvider>
						<Story />
					</SuiClientProvider>
				</QueryClientProvider>
			</MemoryRouter>
		),
	],
} as Meta;

const variants: ObjectVideoImageProps['variant'][] = ['xs', 'small', 'medium', 'large', 'xl'];

export const Default: StoryObj<ObjectVideoImageProps> = {
	render: (props) => (
		<div className="flex flex-col gap-2">
			{variants.map((variant: ObjectVideoImageProps['variant']) => (
				<ObjectVideoImage key={variant} {...props} variant={variant} />
			))}
		</div>
	),
	args: {
		title: 'Test Image Title',
		subtitle: 'Test Subtitle',
		src: 'https://ipfs.io/ipfs/QmTwPRCH4xpTzn7ArJDMvqSgzkuK3AVhTgGGpC7LFRfAGU',
	},
};

export const Portrait: StoryObj<ObjectVideoImageProps> = {
	render: (props) => (
		<div className="flex flex-col gap-2">
			{variants.map((variant: ObjectVideoImageProps['variant']) => (
				<ObjectVideoImage dynamicOrientation key={variant} {...props} variant={variant} />
			))}
		</div>
	),
	args: {
		title: 'Test Image Title',
		subtitle: 'Test Subtitle',
		src: 'https://i.imgur.com/3DtOjtB.jpg',
	},
};

export const Landscape: StoryObj<ObjectVideoImageProps> = {
	render: (props) => (
		<div className="flex flex-col gap-2">
			{variants.map((variant: ObjectVideoImageProps['variant']) => (
				<ObjectVideoImage dynamicOrientation key={variant} {...props} variant={variant} />
			))}
		</div>
	),
	args: {
		title: 'Test Image Title',
		subtitle: 'Test Subtitle',
		src: 'https://www.morrisanimalinn.com/wp-content/uploads/shutterstock_679338581.jpg',
	},
};
