// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { isBasePayload } from '_payloads';

import type { Permission } from './Permission';
import type { BasePayload, Payload } from '_payloads';

export interface PermissionRequests extends BasePayload {
    type: 'permission-request';
    permissions: Permission[];
}

export function isPermissionRequests(
    payload: Payload
): payload is PermissionRequests {
    return isBasePayload(payload) && payload.type === 'permission-request';
}
