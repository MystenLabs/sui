// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
	printWidth: 100,
	semi: true,
	singleQuote: true,
	tabWidth: 2,
	trailingComma: 'all',
	useTabs: true,
	overrides: [
		{
			files: 'apps/explorer/**/*',
			options: {
				plugins: ['prettier-plugin-tailwindcss'],
				tailwindConfig: './apps/explorer/tailwind.config.ts',
			},
		},
	],
};
