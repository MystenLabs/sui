// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const container = style({
	padding: 8,
	borderRadius: 8,
});

export const selectedContainer = style({
	background: '#FFFFFF',
	boxShadow: '0px 2px 6px rgba(0, 0, 0, 0.05)',
});

export const buttonContainer = style({
	display: 'flex',
	alignItems: 'center',
	gap: '8px',
});

export const walletName = style({});

export const walletIcon = style({
	width: 28,
	height: 28,
	borderRadius: 6,
	flexShrink: 0,
	background: 'white',
	objectFit: 'cover',
});
