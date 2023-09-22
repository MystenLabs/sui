// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const testStyle = style({
	padding: 10,
});

export const modalOverlay = style({
	backgroundColor: 'rgba(24 36 53 / 20%)',
	position: 'fixed',
	inset: 0,
	zIndex: 999999999,
});

export const modalContent = style({
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

export const walletListContainer = style({
	backgroundColor: '#F7F8F8',
	padding: 20,
	minWidth: 240,
});
