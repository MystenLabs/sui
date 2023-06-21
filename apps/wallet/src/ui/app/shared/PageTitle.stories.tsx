// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import PageTitle from './PageTitle';

export default {
	component: PageTitle,
} as Meta<typeof PageTitle>;

export const Default: StoryObj<typeof PageTitle> = {
	args: {
		title: 'Title',
	},
};

export const BackUrl: StoryObj<typeof PageTitle> = {
	args: {
		title: 'Title',
		back: '/',
	},
};

export const BackCallback: StoryObj<typeof PageTitle> = {
	args: {
		title: 'Title',
		back: () => alert('Back clicked'),
	},
};

export const BackTrue: StoryObj<typeof PageTitle> = {
	args: {
		title: 'Title',
		back: true,
	},
};

export const BackLongTitle: StoryObj<typeof PageTitle> = {
	args: {
		title: 'AVeryVeryVeeeeeeeeryLoooooongTitle',
		back: true,
	},
};
