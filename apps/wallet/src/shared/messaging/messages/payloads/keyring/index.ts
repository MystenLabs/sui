// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedSignature, type ExportedKeypair } from '@mysten/sui.js/cryptography';
import { isBasePayload } from '_payloads';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';

import type { BasePayload, Payload } from '_payloads';
import type { SerializedAccount } from '_src/background/keyring/Account';

type MethodToPayloads = {
	/**
	 * @deprecated
	 */
	create: {
		args: { password: string; importedEntropy?: string };
		return: void;
	};
	/**
	 * @deprecated
	 */
	getEntropy: {
		args: string | undefined;
		return: string;
	};
	/**
	 * @deprecated
	 */
	unlock: {
		args: { password: string };
		return: void;
	};
	/**
	 * @deprecated
	 */
	walletStatusUpdate: {
		args: void;
		return: {
			isLocked: boolean;
			isInitialized: boolean;
			accounts: SerializedAccount[];
			activeAddress: string | null;
		};
	};
	/**
	 * @deprecated
	 */
	lock: {
		args: void;
		return: void;
	};
	/**
	 * @deprecated
	 */
	clear: {
		args: void;
		return: void;
	};
	/**
	 * @deprecated
	 */
	signData: {
		args: { data: string; address: string };
		return: SerializedSignature;
	};
	/**
	 * @deprecated
	 */
	deriveNextAccount: {
		args: void;
		return: { accountAddress: string };
	};
	/**
	 * @deprecated
	 */
	importLedgerAccounts: {
		args: { ledgerAccounts: SerializedLedgerAccount[] };
		return: void;
	};
	/**
	 * @deprecated
	 */
	verifyPassword: {
		args: { password: string };
		return: void;
	};
	/**
	 * @deprecated
	 */
	exportAccount: {
		args: { password: string; accountAddress: string };
		return: { keyPair: ExportedKeypair };
	};
	/**
	 * @deprecated
	 */
	importPrivateKey: {
		args: { password: string; keyPair: ExportedKeypair };
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
