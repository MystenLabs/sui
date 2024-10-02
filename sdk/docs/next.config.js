// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const withNextra = require('nextra')({
	theme: 'nextra-theme-docs',
	themeConfig: './theme.config.jsx',
});

module.exports = withNextra({
	redirects: () => {
		return [
			{
				source: '/',
				destination: '/typescript',
				statusCode: 302,
			},
			{
				source: '/dapp-kit/zksend',
				destination: '/dapp-kit/stashed',
				statusCode: 302,
			},
		];
	},
});
