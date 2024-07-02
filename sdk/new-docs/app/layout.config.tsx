// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { BaseLayoutProps, DocsLayoutProps } from 'fumadocs-ui/layout';

import { pageTree } from '@/app/source';

// shared configuration
export const baseOptions: BaseLayoutProps = {
	nav: {
		title: 'Sui TypeScript Docs',
	},
	links: [
		{
			text: 'Documentation',
			url: '/docs',
			active: 'nested-url',
		},
	],
};

// docs layout configuration
export const docsOptions: DocsLayoutProps = {
	...baseOptions,
	tree: pageTree,
};
