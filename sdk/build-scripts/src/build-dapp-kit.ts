#! /usr/bin/env tsx
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { vanillaExtractPlugin } from '@vanilla-extract/esbuild-plugin';
import autoprefixer from 'autoprefixer';
import postcss from 'postcss';
import prefixSelector from 'postcss-prefix-selector';

import { buildPackage } from './utils/buildPackage.js';

buildPackage({
	plugins: [
		vanillaExtractPlugin({
			async processCss(css) {
				const result = await postcss([
					autoprefixer,
					prefixSelector({
						prefix: '[data-dapp-kit]',
						transform: (prefix, selector, prefixedSelector) => {
							// Our prefix is applied to all top-level elements rendered to the DOM, so we want
							// our transform to apply to the top-level element itself and all of its children
							// Example: [data-dapp-kit].ConnectModal, [data-dapp-kit] .ConnectModal
							return `${prefix}${selector}, ${prefixedSelector}`;
						},
					}),
				]).process(css, {
					// Suppress source map warning
					from: undefined,
				});
				return result.css;
			},
		}),
	],
	packages: 'external',
	bundle: true,
}).catch((error) => {
	console.error(error);
	process.exit(1);
});
