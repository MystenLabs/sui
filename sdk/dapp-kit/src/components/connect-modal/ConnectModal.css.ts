// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const overlay = style({
	backgroundColor: 'rgba(24 36 53 / 20%)',
	position: 'fixed',
	inset: 0,
	zIndex: 999999999,
});

export const content = style({
	backgroundColor: 'white',
	position: 'fixed',
	top: '50%',
	left: '50%',
	transform: 'translate(-50%, -50%)',
	zIndex: 999999999,
	display: 'flex',
	overflow: 'hidden',
	borderRadius: 16,
});

export const triggerButton = style({});

export const closeButton = style({
	position: 'absolute',
	padding: 7,
	top: 16,
	right: 16,
	borderRadius: 9999,
	backgroundColor: '#F0F1F2',
});

export const walletListContainer = style({
	backgroundColor: '#F7F8F8',
	padding: 20,
	minWidth: 240,
});
