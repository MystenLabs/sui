// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import tsconfigPaths from 'vite-tsconfig-paths';

import docRender from './src/plugins/doc-render';

// https://vitejs.dev/config/
export default defineConfig({
	plugins: [
		react(),
		tsconfigPaths(),
		{
			name: 'doc-data',
			resolveId(id) {
				if (id === '@doc-data') {
					return id;
				}
			},
			load(id) {
				if (id === '@doc-data') {
					const data = docRender();
					return `export default ${JSON.stringify(data.content)}`;
				}
			},
		},
	],
});
