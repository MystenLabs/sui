// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { globalStyle } from '@vanilla-extract/css';

import { styleDataAttributeSelector } from '../../constants/styleDataAttribute.js';

globalStyle(createScopedSelector('*'), {
	boxSizing: 'border-box',
});

globalStyle(createScopedSelector('button'), {
	appearance: 'none',
	backgroundColor: 'transparent',
	fontSize: 'inherit',
	fontFamily: 'inherit',
	lineHeight: 'inherit',
	letterSpacing: 'inherit',
	outline: 'none',
	color: 'inherit',
	border: 0,
	padding: 0,
	margin: 0,
});

globalStyle(createScopedSelector('a'), {
	textDecoration: 'none',
	color: 'inherit',
	outline: 'none',
});

globalStyle(createScopedSelector('ol, ul'), {
	listStyle: 'none',
	margin: 0,
	padding: 0,
});

globalStyle(createScopedSelector('h1, h2, h3, h4, h5, h6'), {
	fontSize: 'inherit',
	fontWeight: 'inherit',
	margin: 0,
});

function createScopedSelector(selector: string) {
	return `${styleDataAttributeSelector}:where(${selector}), ${styleDataAttributeSelector} :where(${selector})`;
}
