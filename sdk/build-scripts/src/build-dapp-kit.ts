#! /usr/bin/env tsx
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { buildPackage } from './utils/buildPackage';
import { vanillaExtractPlugin } from '@vanilla-extract/esbuild-plugin';

buildPackage({
	plugins: [vanillaExtractPlugin()],
	packages: 'external',
	bundle: true,
}).catch((error) => {
	console.error(error);
	process.exit(1);
});
