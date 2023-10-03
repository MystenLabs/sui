// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RecipeVariants } from '@vanilla-extract/recipes';
import { recipe } from '@vanilla-extract/recipes';

import { themeVars } from '../../themes/themeContract.js';

export const headingVariants = recipe({
	variants: {
		size: {
			'1': {
				fontSize: 14,
			},
			'2': {
				fontSize: 16,
			},
			'3': {
				fontSize: 18,
			},
			'4': {
				fontSize: 20,
			},
		},
		weight: {
			normal: { fontWeight: themeVars.fontWeights.normal },
			bold: { fontWeight: themeVars.fontWeights.bold },
		},
		truncate: {
			true: {
				overflow: 'hidden',
				textOverflow: 'ellipsis',
				whiteSpace: 'nowrap',
			},
		},
	},
	defaultVariants: {
		size: '3',
		weight: 'bold',
	},
});

export type HeadingVariants = RecipeVariants<typeof headingVariants>;
