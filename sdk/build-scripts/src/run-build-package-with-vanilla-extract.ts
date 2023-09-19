#! /usr/bin/env tsx
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { buildPackage } from './build-package';
import { vanillaExtractPlugin } from '@vanilla-extract/esbuild-plugin';

buildPackage({
	plugins: [
		vanillaExtractPlugin(),
		{
			name: 'make-all-packages-external',
			setup(build) {
				let filter = /^[^./]|^\.[^./]|^\.\.[^/]/; // Must not start with "/" or "./" or "../"
				build.onResolve({ filter }, (args) => ({
					external: true,
					path: args.path,
				}));
			},
		},
	],
	bundle: true,
}).catch((error) => {
	console.error(error);
	process.exit(1);
});
