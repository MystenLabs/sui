// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';
import type { ApprovalRequest } from '_payloads/transactions/ApprovalRequest';

export interface GetTransactionRequestsResponse extends BasePayload {
	type: 'get-transaction-requests-response';
	txRequests: ApprovalRequest[];
}

export function isGetTransactionRequestsResponse(
	payload: Payload,
): payload is GetTransactionRequestsResponse {
	return isBasePayload(payload) && payload.type === 'get-transaction-requests-response';
}
