// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ImageIcon, type ImageIconProps } from '../ImageIcon';

import type { Meta, StoryObj } from '@storybook/react';

export default {
	component: ImageIcon,
} as Meta;

export const extraLargeImage: StoryObj<ImageIconProps> = {
	args: {
		src: 'https://ipfs.io/ipfs/QmZPWWy5Si54R3d26toaqRiqvCH7HkGdXkxwUgCm2oKKM2?filename=img-sq-01.png',
		alt: 'Blockdaemon',
		size: 'xl',
	},
};

export const largeIconNoImage: StoryObj<ImageIconProps> = {
	args: {
		src: null,
		fallback: 'Sui',
		label: 'Sui',
		size: 'lg',
	},
};

export const smallIconImage: StoryObj<ImageIconProps> = {
	args: {
		src: 'https://ipfs.io/ipfs/QmZPWWy5Si54R3d26toaqRiqvCH7HkGdXkxwUgCm2oKKM2?filename=img-sq-01.png',
		label: 'Sui',
		size: 'sm',
		fallback: 'Sui',
	},
};
