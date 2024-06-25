// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum RequestType {
	CONNECT = 'connect',
	SIGN_TRANSACTION = 'sign-transaction',
	SIGN_MESSAGE = 'sign-personal-message',
}

export type DappPreflightResponse = {
	block: {
		enabled: boolean;
		title: string;
		subtitle: string;
	};
};

export type DappPreflightRequest = {
	requestType: RequestType;
	origin: string;
	transactionBytes?: string;
	message?: string;
};
