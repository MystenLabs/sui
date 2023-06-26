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
});
