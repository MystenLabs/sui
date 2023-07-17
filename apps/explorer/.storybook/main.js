// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
	stories: [
		{
			directory: '../src/ui/stories',
			titlePrefix: 'UI',
			files: '**/*.stories.*',
		},
	],
	addons: ['@storybook/addon-a11y', '@storybook/addon-essentials'],
	framework: '@storybook/react-vite',
	staticDirs: ['../public'],
};
