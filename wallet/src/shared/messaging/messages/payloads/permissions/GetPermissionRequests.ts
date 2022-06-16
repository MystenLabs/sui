import type { BasePayload } from '_messages/payloads/BasePayload';

export interface GetPermissionRequests extends BasePayload {
    type: 'get-permission-requests';
}
