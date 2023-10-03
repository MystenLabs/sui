// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

import { themeVars } from '../../themes/themeContract.js';

export const container = style({
	display: 'flex',
	flexDirection: 'column',
	gap: 4,
});

export const description = style({
	color: themeVars.colors.bodyMuted,
});
