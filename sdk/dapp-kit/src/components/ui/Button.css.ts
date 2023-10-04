// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RecipeVariants } from '@vanilla-extract/recipes';
import { recipe } from '@vanilla-extract/recipes';

import { themeVars } from '../../themes/themeContract.js';

export const buttonVariants = recipe({
	base: {
		display: 'inline-flex',
		alignItems: 'center',
		justifyContent: 'center',
		fontWeight: themeVars.fontWeights.medium,
		':disabled': {
			opacity: 0.5,
		},
	},
	variants: {
		variant: {
			primary: {
				backgroundColor: themeVars.backgroundColors.primaryButton,
				color: themeVars.colors.primaryButton,
				boxShadow: themeVars.shadows.primaryButton,
				':hover': {
					backgroundColor: themeVars.backgroundColors.primaryButtonHover,
				},
			},
			outline: {
				borderWidth: 1,
				borderStyle: 'solid',
				borderColor: themeVars.borderColors.outlineButton,
				color: themeVars.colors.outlineButton,
				':hover': {
					backgroundColor: themeVars.backgroundColors.outlineButtonHover,
				},
			},
		},
		size: {
			md: { borderRadius: themeVars.radii.medium, padding: '8px 16px' },
			lg: { borderRadius: themeVars.radii.large, padding: '16px 24px ' },
		},
	},
	defaultVariants: {
		variant: 'primary',
		size: 'md',
	},
});

export type ButtonVariants = RecipeVariants<typeof buttonVariants>;
