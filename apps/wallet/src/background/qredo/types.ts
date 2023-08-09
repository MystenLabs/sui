// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Wallet } from '_src/shared/qredo-api';

export type QredoConnectIdentity = {
	service: string;
	apiUrl: string;
	origin: string;
	organization: string;
};

export type QredoConnectPendingRequest = {
	id: string;
	originFavIcon?: string;
	token: string;
	windowID: number | null;
	messageIDs: string[];
	accessToken: string | null;
} & QredoConnectIdentity;

export type UIQredoPendingRequest = Pick<
	QredoConnectPendingRequest,
	'id' | 'service' | 'apiUrl' | 'origin' | 'originFavIcon' | 'organization'
> & { partialToken: `…${string}` };

export type UIQredoInfo = {
	id: string;
	accessToken: string | null;
	apiUrl: string;
	service: string;
	accounts: Wallet[];
};

export type QredoConnection = Omit<
	QredoConnectPendingRequest,
	'token' | 'windowID' | 'messageIDs'
> & {
	accounts: Wallet[];
	accessToken: string | null;
};
