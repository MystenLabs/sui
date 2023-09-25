// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'vite';
import { vanillaExtractPlugin } from '@vanilla-extract/vite-plugin';

process.env.VITE_VERCEL_ENV = process.env.VERCEL_ENV || 'development';

// https://vitejs.dev/config/
export default defineConfig({
	plugins: [vanillaExtractPlugin()],
	resolve: {
		conditions: ['source'],
	},
});
