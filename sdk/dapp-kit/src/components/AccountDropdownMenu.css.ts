// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const triggerButton = style({
	display: 'inline-flex',
	justifyContent: 'space-between',
	alignItems: 'center',
	gap: 8,
	paddingLeft: 24,
	paddingRight: 24,
	paddingTop: 16,
	paddingBottom: 16,
	borderRadius: 12,
	boxShadow: '0px 4px 12px rgba(0, 0, 0, 0.1)',
	backgroundColor: 'white',
	color: '#182435',
});

export const menuContent = style({
	width: 180,
	maxHeight: 200,
	borderRadius: 12,
	marginTop: 4,
	padding: 8,
	display: 'flex',
	flexDirection: 'column',
	gap: 8,
	backgroundColor: 'white',
});

export const switchAccountButton = style({
	display: 'flex',
	justifyContent: 'space-between',
	alignItems: 'center',
});

export const disconnectButton = style({
	display: 'flex',
	justifyContent: 'space-between',
	alignItems: 'center',
});

export const separator = style({
	height: 1,
	backgroundColor: '#F3F6F8',
});
