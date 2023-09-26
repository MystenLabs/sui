// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createMessage } from '_messages';
import type { Message } from '_messages';
import type { ErrorPayload } from '_payloads';
import { isKeyringPayload } from '_payloads/keyring';
import mitt from 'mitt';

import type { UiConnection } from '../connections/UiConnection';
import { getFromLocalStorage } from '../storage-utils';
import { type Account } from './Account';
import { type SerializedLedgerAccount } from './LedgerAccount';
import { VaultStorage } from './VaultStorage';

/** The key for the extension's storage, that holds the index of the last derived account (zero based) */
export const STORAGE_LAST_ACCOUNT_INDEX_KEY = 'last_account_index';

export const STORAGE_IMPORTED_LEDGER_ACCOUNTS = 'imported_ledger_accounts';

type KeyringEvents = {
	lockedStatusUpdate: boolean;
	accountsChanged: Account[];
	activeAccountChanged: string;
};

export async function getSavedLedgerAccounts() {
	const ledgerAccounts = await getFromLocalStorage<SerializedLedgerAccount[]>(
		STORAGE_IMPORTED_LEDGER_ACCOUNTS,
		[],
	);
	return ledgerAccounts || [];
}

/**
 * @deprecated
 */
// exported to make testing easier the default export should be used
export class Keyring {
	#events = mitt<KeyringEvents>();
	#locked = true;

	public get isLocked() {
		return this.#locked;
	}

	public on = this.#events.on;

	public off = this.#events.off;

	public async handleUiMessage(msg: Message, uiConnection: UiConnection) {
		const { id, payload } = msg;
		try {
			if (isKeyringPayload(payload, 'verifyPassword') && payload.args) {
				if (!(await VaultStorage.verifyPassword(payload.args.password))) {
					throw new Error('Wrong password');
				}
				uiConnection.send(createMessage({ type: 'done' }, id));
			}
		} catch (e) {
			uiConnection.send(
				createMessage<ErrorPayload>({ code: -1, error: true, message: (e as Error).message }, id),
			);
		}
	}
}

const keyring = new Keyring();

/**
 * @deprecated
 */
export default keyring;
