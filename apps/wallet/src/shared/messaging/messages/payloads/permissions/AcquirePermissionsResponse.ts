// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface AcquirePermissionsResponse extends BasePayload {
	type: 'acquire-permissions-response';
	result: boolean;
}

export function isAcquirePermissionsResponse(
	payload: Payload,
): payload is AcquirePermissionsResponse {
	return isBasePayload(payload) && payload.type === 'acquire-permissions-response';
}
