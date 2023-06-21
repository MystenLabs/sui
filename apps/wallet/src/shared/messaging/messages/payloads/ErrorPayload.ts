// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Payload } from './Payload';

export interface ErrorPayload {
	error: true;
	code: number;
	message: string;
}

export function isErrorPayload(payload: Payload): payload is ErrorPayload {
	return 'error' in payload && payload.error === true;
}
