// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createGlobalThemeContract } from '@vanilla-extract/css';

const themeContractValues = {
	blurs: {
		modalOverlay: '',
	},
	backgroundColors: {
		primaryButton: '',
		primaryButtonHover: '',
		outlineButtonHover: '',
		walletItemHover: '',
		walletItemSelected: '',
		modalOverlay: '',
		modalPrimary: '',
		modalSecondary: '',
		iconButton: '',
		iconButtonHover: '',
		dropdownMenu: '',
		dropdownMenuSeparator: '',
	},
	borderColors: {
		outlineButton: '',
	},
	colors: {
		primaryButton: '',
		outlineButton: '',
		body: '',
		bodyMuted: '',
		bodyDanger: '',
		iconButton: '',
	},
	radii: {
		small: '',
		medium: '',
		large: '',
		xlarge: '',
	},
	shadows: {
		primaryButton: '',
		walletItemSelected: '',
	},
	fontWeights: {
		normal: '',
		medium: '',
		bold: '',
	},
	fontSizes: {
		small: '',
		medium: '',
		large: '',
		xlarge: '',
	},
	typography: {
		fontFamily: '',
		fontStyle: '',
		lineHeight: '',
		letterSpacing: '',
	},
};

export type ThemeVars = typeof themeContractValues;

/**
 * A custom theme that is enabled when various conditions are
 */
export type DynamicTheme = {
	/**
	 * An optional media query required for the given theme to be enabled. This is useful
	 * when you want the theme of your application to automatically switch depending on
	 * a media feature.
	 *
	 * @example '(prefers-color-scheme: dark)'
	 */
	mediaQuery?: string;

	/**
	 * An optional CSS selector required for the given theme to be enabled. This is useful
	 * when you have a manual theme switcher on your application that sets a top-level
	 * class name or data-attribute to control the current theme.
	 *
	 * @example '.data-dark'
	 */
	selector?: string;

	/** The theme definitions that will be set when the selector and mediaQuery criteria are matched. */
	variables: ThemeVars;
};

export type Theme = ThemeVars | DynamicTheme[];

export const themeVars = createGlobalThemeContract(
	themeContractValues,
	(_, path) => `dapp-kit-${path.join('-')}`,
);
