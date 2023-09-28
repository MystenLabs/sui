// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const container = style({
	display: 'flex',
	flexDirection: 'column',
	justifyContent: 'center',
	alignItems: 'center',
	width: '100%',
});

export const walletIcon = style({
	backgroundColor: 'white',
	objectFit: 'cover',
	width: 72,
	height: 72,
	borderRadius: 16,
});

export const walletName = style({
	marginTop: 12,
});

export const connectionStatus = style({
	marginTop: 4,
});

export const connectionStatusWithError = style({
	color: 'red',
});
