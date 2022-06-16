export type PayloadType =
    | 'permission-request'
    | 'permission-response'
    | 'get-permission-requests'
    | 'get-account'
    | 'get-account-response';

export interface BasePayload {
    type: PayloadType;
}
