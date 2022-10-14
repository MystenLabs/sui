// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { Base64DataBuffer } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface ExecuteSignatureRequest extends BasePayload {
  type: 'sign-message-request';
  message: Base64DataBuffer;
}

export function isExecuteSignatureRequest(
  payload: Payload
): payload is ExecuteSignatureRequest {
  return (
    isBasePayload(payload) && payload.type === 'sign-message-request'
  );
}
