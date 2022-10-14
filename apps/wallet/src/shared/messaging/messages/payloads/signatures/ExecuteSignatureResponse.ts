// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SignaturePubkeyPair } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface ExecuteSignatureResponse extends BasePayload {
  type: 'sign-message-response';
  signature: SignaturePubkeyPair;
  responseDate: string;
}

export function isExecuteSignatureResponse(
  payload: Payload
): payload is ExecuteSignatureResponse {
  return (
    isBasePayload(payload) &&
    payload.type === 'sign-message-response'
  );
}
