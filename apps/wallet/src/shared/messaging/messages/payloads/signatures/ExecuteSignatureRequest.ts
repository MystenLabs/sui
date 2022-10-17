// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  isBasePayload,
  type BasePayload,
  type Payload
} from '_payloads';

export interface ExecuteSignatureRequest extends BasePayload {
  type: 'sign-message-request';
  message: Uint8Array;
}

export function isExecuteSignatureRequest(
  payload: Payload
): payload is ExecuteSignatureRequest {
  return (
    isBasePayload(payload) && payload.type === 'sign-message-request'
  );
}
