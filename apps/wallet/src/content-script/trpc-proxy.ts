// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { runtime } from 'webextension-polyfill';

/**
 * This sets up a simple proxy conneciton between the injected script and the content
 * script which allows the injected script to use a TRPC client.
 */
export function setupTRPCProxy() {
    const port = runtime.connect({ name: 'trpc' });

    const onDisconnect = () => {
        cleanup();
        setupTRPCProxy();
        window.dispatchEvent(new CustomEvent('trpc-reconnect'));
    };

    const onMessage = (data: unknown) => {
        window.dispatchEvent(
            new CustomEvent('trpc-response', { detail: data })
        );
    };

    const requestHandler = (event: Event) => {
        if (event instanceof CustomEvent) {
            port.postMessage(event.detail);
        }
    };

    port.onDisconnect.addListener(onDisconnect);
    port.onMessage.addListener(onMessage);
    window.addEventListener('trpc-request', requestHandler);

    function cleanup() {
        port.onDisconnect.removeListener(onDisconnect);
        port.onMessage.removeListener(onMessage);
        window.removeEventListener('trpc-request', requestHandler);
    }
}
