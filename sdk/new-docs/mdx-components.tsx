// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import defaultComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';

export function useMDXComponents(components: MDXComponents): MDXComponents {
	return {
		...defaultComponents,
		...components,
	};
}
