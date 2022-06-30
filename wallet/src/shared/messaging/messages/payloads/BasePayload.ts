// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Payload } from './Payload';

export type PayloadType =
    | 'permission-request'
    | 'permission-response'
    | 'get-permission-requests'
    | 'get-account'
    | 'get-account-response';

export interface BasePayload {
    type: PayloadType;
}

export function isBasePayload(payload: Payload): payload is BasePayload {
    return 'type' in payload && typeof payload.type !== 'undefined';
}
