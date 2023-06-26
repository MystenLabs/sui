// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LoadingSpinner, type LoadingSpinnerProps } from '../LoadingSpinner';

import type { Meta, StoryObj } from '@storybook/react';

export default {
	component: LoadingSpinner,
} as Meta;

export const LoadingSpinnerDefault: StoryObj<LoadingSpinnerProps> = {
	render: (props) => <LoadingSpinner {...props} />,
};

export const LoadingSpinnerWithText: StoryObj<LoadingSpinnerProps> = {
	...LoadingSpinnerDefault,
	args: {
		text: 'Loading...',
	},
};
