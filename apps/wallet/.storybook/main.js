// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { default: getWebpackConfig } = require('../configs/webpack/webpack.config.dev.ts');
const MiniCssExtractPlugin = require('mini-css-extract-plugin');
const path = require('path');

module.exports = {
	stories: ['../src/ui/**/*.mdx', '../src/ui/**/*.stories.@(js|jsx|ts|tsx)'],
	addons: [
		'@storybook/addon-links',
		'@storybook/addon-essentials',
		'@storybook/addon-interactions',
	],
	framework: {
		name: '@storybook/react-webpack5',
		options: {},
	},
	docs: {
		docsPage: true,
	},
	webpackFinal: async (config) => {
		const custom = await getWebpackConfig();

		config.plugins.push(new MiniCssExtractPlugin());
		config.resolve.alias = custom.resolve.alias;

		const cssRule = custom.module.rules.find((rule) => rule.test?.test('.css'));
		const tsRule = custom.module.rules.find((rule) => rule.test?.test('.tsx'));
		tsRule.include = path.resolve('../icons/src');

		config.module.rules = [
			...config.module.rules.filter((rule) => !rule.test?.test('.css')),
			tsRule,
			cssRule,
		];

		return config;
	},
};
