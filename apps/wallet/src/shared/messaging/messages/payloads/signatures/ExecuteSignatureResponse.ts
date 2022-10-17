// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  isBasePayload,
  type BasePayload,
  type Payload
} from '_payloads';

import type { SuiSignMessageOutput } from '@mysten/wallet-standard';

export interface ExecuteSignatureResponse extends BasePayload, SuiSignMessageOutput {
  type: 'sign-message-response';
}

export function isExecuteSignatureResponse(
  payload: Payload
): payload is ExecuteSignatureResponse {
  return (
    isBasePayload(payload) &&
    payload.type === 'sign-message-response'
  );
}
