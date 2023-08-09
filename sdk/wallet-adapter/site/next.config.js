// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const withNextra = require('nextra')({
	theme: 'nextra-theme-docs',
	themeConfig: './theme.config.tsx',
});

module.exports = withNextra({
	experimental: {
		externalDir: true,
	},
	webpack: (webpackConfig, { webpack }) => {
		// Fix .js imports from @mysten/sui.js since we are importing it from source
		webpackConfig.resolve.extensionAlias = {
			'.js': ['.js', '.ts'],
			'.mjs': ['.mjs', '.mts'],
			'.cjs': ['.cjs', '.cts'],
		};
		return webpackConfig;
	},
});
