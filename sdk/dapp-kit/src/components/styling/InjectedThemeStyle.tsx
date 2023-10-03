// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { assignInlineVars } from '@vanilla-extract/dynamic';

import { styleDataAttributeSelector } from '../../constants/styleDataAttribute.js';
import { themeVars } from '../../themes/themeContract.js';
import type { Theme, ThemeVars } from '../../themes/themeContract.js';

type InjectedThemeStyleProps = {
	theme: Theme;
};

export function InjectedThemeStyle({ theme }: InjectedThemeStyleProps) {
	const defaultStyles = `${styleDataAttributeSelector}{${cssStringFromTheme(
		'light' in theme ? theme.light : theme,
	)}}`;
	const darkModeStyles =
		'dark' in theme
			? `@media(prefers-color-scheme:dark){${styleDataAttributeSelector}{${cssStringFromTheme(
					theme.dark,
			  )}}}`
			: null;

	return (
		<style
			dangerouslySetInnerHTML={{
				__html: [defaultStyles, darkModeStyles].join(''),
			}}
		/>
	);
}

function cssStringFromTheme(theme: ThemeVars) {
	return Object.entries(assignInlineVars(themeVars, theme))
		.map(([key, value]) => `${key}:${value};`)
		.join('');
}
