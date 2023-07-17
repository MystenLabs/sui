// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'vite';

process.env.VITE_VERCEL_ENV = process.env.VERCEL_ENV || 'development';

// https://vitejs.dev/config/
export default defineConfig({
	resolve: {
		conditions: ['source'],
	},
});
