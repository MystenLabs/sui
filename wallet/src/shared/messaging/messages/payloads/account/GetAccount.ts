import type { BasePayload } from '_messages/payloads/BasePayload';

export interface GetAccount extends BasePayload {
    type: 'get-account';
}
