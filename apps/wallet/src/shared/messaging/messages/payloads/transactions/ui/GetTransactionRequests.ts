// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface GetTransactionRequests extends BasePayload {
	type: 'get-transaction-requests';
}

export function isGetTransactionRequests(payload: Payload): payload is GetTransactionRequests {
	return isBasePayload(payload) && payload.type === 'get-transaction-requests';
}
