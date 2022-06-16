import type { Permission } from './Permission';
import type { BasePayload } from '_messages/payloads/BasePayload';

export interface PermissionRequests extends BasePayload {
    type: 'permission-request';
    permissions: Permission[];
}
