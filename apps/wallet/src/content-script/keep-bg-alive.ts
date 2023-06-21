// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import type { Runtime } from 'webextension-polyfill';

export const KEEP_ALIVE_BG_PORT_NAME = 'content-script<->background-script';
export const MSG_CONNECT = 'connect';
export const MSG_DISABLE_AUTO_RECONNECT = 'disable-auto-reconnect';

let bgPort: Runtime.Port | null = null;
let autoReConnect = true;

function doConnect() {
	if (autoReConnect) {
		try {
			bgPort = Browser.runtime.connect({ name: KEEP_ALIVE_BG_PORT_NAME });
		} catch (e) {
			// usually fails when extension gets updated and context is invalidated
		}
	}
	if (bgPort) {
		bgPort.onMessage.addListener((msg) => {
			if (msg === MSG_DISABLE_AUTO_RECONNECT) {
				autoReConnect = false;
			}
		});
		bgPort.onDisconnect.addListener(() => {
			bgPort = null;
			doConnect();
		});
	}
}

export function init() {
	Browser.runtime.onMessage.addListener((msg) => {
		if (msg === MSG_CONNECT) {
			autoReConnect = true;
			if (!bgPort) {
				doConnect();
			}
		}
	});
	doConnect();
}
