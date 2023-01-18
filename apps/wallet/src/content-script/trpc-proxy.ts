import { runtime } from 'webextension-polyfill';

/**
 * This sets up a simple proxy conneciton between the injected script and the content
 * script which allows the injected script to use a TRPC client.
 */
export function setupTRPCProxy() {
    // TODO: Handle ports being disconnected seamlessly.
    const port = runtime.connect({ name: 'trpc' });

    port.onMessage.addListener((data) => {
        window.dispatchEvent(
            new CustomEvent('trpc-response', { detail: data })
        );
    });

    window.addEventListener('trpc-request', (event) => {
        if (event instanceof CustomEvent) {
            port.postMessage(event.detail);
        }
    });
}
