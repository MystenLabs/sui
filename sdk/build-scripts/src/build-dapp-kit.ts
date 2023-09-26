#! /usr/bin/env tsx
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { vanillaExtractPlugin } from '@vanilla-extract/esbuild-plugin';
import autoprefixer from 'autoprefixer';
import postcss from 'postcss';

import { buildPackage } from './utils/buildPackage';

buildPackage({
	plugins: [
		vanillaExtractPlugin({
			async processCss(css) {
				const result = await postcss([autoprefixer]).process(css, {
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
