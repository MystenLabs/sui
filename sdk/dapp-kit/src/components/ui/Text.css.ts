// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RecipeVariants } from '@vanilla-extract/recipes';
import { recipe } from '@vanilla-extract/recipes';

import { themeVars } from '../../themes/themeContract.js';

export const textVariants = recipe({
	variants: {
		size: {
			sm: {
				fontSize: themeVars.fontSizes.small,
			},
		},
		weight: {
			normal: { fontWeight: themeVars.fontWeights.normal },
			medium: { fontWeight: themeVars.fontWeights.medium },
			bold: { fontWeight: themeVars.fontWeights.bold },
		},
		color: {
			muted: { color: themeVars.colors.bodyMuted },
			danger: { color: themeVars.colors.bodyDanger },
		},
		mono: {
			true: {
				fontFamily:
					'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
			},
		},
	},
	defaultVariants: {
		size: 'sm',
		weight: 'normal',
	},
});

export type TextVariants = RecipeVariants<typeof textVariants>;
