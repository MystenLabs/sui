// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RecipeVariants } from '@vanilla-extract/recipes';
import { recipe } from '@vanilla-extract/recipes';

import { themeVars } from '../../themes/themeContract.js';

export const headingVariants = recipe({
	variants: {
		size: {
			sm: {
				fontSize: themeVars.fontSizes.small,
			},
			md: {
				fontSize: themeVars.fontSizes.medium,
			},
			lg: {
				fontSize: themeVars.fontSizes.large,
			},
			xl: {
				fontSize: themeVars.fontSizes.xlarge,
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
		size: 'lg',
		weight: 'bold',
	},
});

export type HeadingVariants = RecipeVariants<typeof headingVariants>;
