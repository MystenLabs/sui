import type { SuiAddress } from '@mysten/sui.js';
import type { BasePayload } from '_messages/payloads/BasePayload';

export interface PermissionResponse extends BasePayload {
    type: 'permission-response';
    id: string;
    accounts: SuiAddress[];
    allowed: boolean;
    responseDate: string;
}

export function isPermissionResponse(
    payload: BasePayload
): payload is PermissionResponse {
    return payload.type === 'permission-response';
}
