import { type BasePayload, isBasePayload } from './BasePayload';
import { type Payload } from './Payload';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';

type methods = {
    connect: QredoConnectInput;
    connectResponse: { allowed: boolean };
};

export interface QredoConnectPayload<M extends keyof methods>
    extends BasePayload {
    type: 'qredo-connect';
    method: M;
    args: methods[M];
}

export function isQredoConnectPayload<M extends keyof methods>(
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
