// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type BasePayload, isBasePayload } from './BasePayload';
import { type Payload } from './Payload';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';

type Methods = {
    connect: QredoConnectInput;
    connectResponse: { allowed: boolean };
};

export interface QredoConnectPayload<M extends keyof Methods>
    extends BasePayload {
    type: 'qredo-connect';
    method: M;
    args: Methods[M];
}

export function isQredoConnectPayload<M extends keyof Methods>(
    payload: Payload,
    method: M
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
