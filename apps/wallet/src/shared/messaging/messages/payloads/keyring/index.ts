// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';
import type { BasePayload, Payload } from '_payloads';

type MethodToPayloads = {
	/**
	 * @deprecated
	 */
	verifyPassword: {
		args: { password: string };
		return: void;
	};
};

/**
 * @deprecated
 */
export interface KeyringPayload<Method extends keyof MethodToPayloads> extends BasePayload {
	type: 'keyring';
	method: Method;
	args?: MethodToPayloads[Method]['args'];
	return?: MethodToPayloads[Method]['return'];
}

/**
 * @deprecated
 */
export function isKeyringPayload<Method extends keyof MethodToPayloads>(
	payload: Payload,
	method: Method,
): payload is KeyringPayload<Method> {
	return (
		isBasePayload(payload) &&
		payload.type === 'keyring' &&
		'method' in payload &&
		payload['method'] === method
	);
}
