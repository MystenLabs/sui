// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type QredoSerializedUiAccount } from '_src/background/accounts/QredoAccount';
import { type UIQredoInfo, type UIQredoPendingRequest } from '_src/background/qredo/types';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';
import { type Wallet } from '_src/shared/qredo-api';

import { isBasePayload, type BasePayload } from './BasePayload';
import { type Payload } from './Payload';

type Methods = {
	connect: QredoConnectInput;
	connectResponse: { allowed: boolean };
	getPendingRequest: { requestID: string };
	getPendingRequestResponse: { request: UIQredoPendingRequest | null };
	getQredoInfo: {
		qredoID: string;
		refreshAccessToken: boolean;
	};
	getQredoInfoResponse: { qredoInfo: UIQredoInfo | null };
	acceptQredoConnection: {
		qredoID: string;
		accounts: Wallet[];
		password: string;
	};
	acceptQredoConnectionResponse: { accounts: QredoSerializedUiAccount[] };
	rejectQredoConnection: {
		qredoID: string;
	};
};

export interface QredoConnectPayload<M extends keyof Methods> extends BasePayload {
	type: 'qredo-connect';
	method: M;
	args: Methods[M];
}

export function isQredoConnectPayload<M extends keyof Methods>(
	payload: Payload,
	method: M,
): payload is QredoConnectPayload<M> {
	return (
		isBasePayload(payload) &&
		payload.type === 'qredo-connect' &&
		'method' in payload &&
		payload.method === method &&
		'args' in payload &&
		!!payload.args
	);
}
