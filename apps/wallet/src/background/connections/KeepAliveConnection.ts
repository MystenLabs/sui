// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BehaviorSubject, filter, map, take } from 'rxjs';

import Keyring from '_src/background/keyring';
import { isSessionStorageSupported } from '_src/background/storage-utils';
import { MSG_DISABLE_AUTO_RECONNECT } from '_src/content-script/keep-bg-alive';

import type { Runtime } from 'webextension-polyfill';

const MIN_DISCONNECT_TIMEOUT = 1000 * 30;
const MAX_DISCONNECT_TIMEOUT = 1000 * 60 * 3;

export class KeepAliveConnection {
	private onDisconnectSubject = new BehaviorSubject<Runtime.Port | null>(null);
	private autoDisconnectTimeout: number | null = null;
	private port: Runtime.Port;

	constructor(port: Runtime.Port) {
		this.port = port;
		if (isSessionStorageSupported()) {
			this.forcePortDisconnect(false);
			return;
		}
		Keyring.on('lockedStatusUpdate', this.onKeyringLockedStatusUpdate);
		this.port.onDisconnect.addListener(this.onPortDisconnected);
		this.onKeyringLockedStatusUpdate(Keyring.isLocked);
	}

	public get onDisconnect() {
		return this.onDisconnectSubject.asObservable().pipe(
			filter((aPort) => !!aPort),
			map((port) => ({
				port,
			})),
			take(1),
		);
	}

	private getRandomDisconnectTimeout() {
		return Math.floor(
			Math.random() * (MAX_DISCONNECT_TIMEOUT - MIN_DISCONNECT_TIMEOUT + 1) +
				MIN_DISCONNECT_TIMEOUT,
		);
	}

	private onPortDisconnected = (aPort: Runtime.Port) => {
		this.clearAutoDisconnectTimeout();
		Keyring.off('lockedStatusUpdate', this.onKeyringLockedStatusUpdate);
		this.onDisconnectSubject.next(aPort);
	};

	private onKeyringLockedStatusUpdate = (isLocked: boolean) => {
		if (isLocked) {
			this.forcePortDisconnect(false);
		} else {
			this.autoDisconnectTimeout = setTimeout(() => {
				this.forcePortDisconnect(true);
				this.autoDisconnectTimeout = null;
			}, this.getRandomDisconnectTimeout()) as unknown as number;
		}
	};

	private clearAutoDisconnectTimeout() {
		if (this.autoDisconnectTimeout) {
			clearTimeout(this.autoDisconnectTimeout);
			this.autoDisconnectTimeout = null;
		}
	}

	private forcePortDisconnect(allowReconnect: boolean) {
		if (!allowReconnect) {
			try {
				this.port.postMessage(MSG_DISABLE_AUTO_RECONNECT);
			} catch (e) {
				// in case port is already closed
			}
		}
		this.port.disconnect();
		// calling disconnect triggers onDisconnect only on the other side of the port
		// so we are calling it ourselves to clean-up
		this.onPortDisconnected(this.port);
	}
}
