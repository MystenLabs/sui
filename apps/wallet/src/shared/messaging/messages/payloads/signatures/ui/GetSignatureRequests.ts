// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface GetSignatureRequests extends BasePayload {
	type: 'get-signature-requests';
}

export function isGetSignatureRequests(
	payload: Payload
): payload is GetSignatureRequests {
	return isBasePayload(payload) && payload.type === 'get-signature-requests';
}
