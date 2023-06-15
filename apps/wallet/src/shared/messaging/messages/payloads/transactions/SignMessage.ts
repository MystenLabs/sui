// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { type SuiSignMessageOutput } from '@mysten/wallet-standard';

import { type BasePayload, isBasePayload } from '../BasePayload';
import { type Payload } from '../Payload';

export interface SignMessageRequest extends BasePayload {
	type: 'sign-message-request';
	args?: {
		message: string; // base64
		accountAddress: SuiAddress;
	};
	return?: SuiSignMessageOutput;
}

export function isSignMessageRequest(payload: Payload): payload is SignMessageRequest {
	return isBasePayload(payload) && payload.type === 'sign-message-request';
}
