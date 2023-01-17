import { createTRPCReact } from '@trpc/react-query';
import { chromeLink } from 'trpc-chrome/link';
import { runtime } from 'webextension-polyfill';

import type { AppRouter } from '_src/background/trpc';

export const trpc = createTRPCReact<AppRouter>();

const port = runtime.connect({ name: 'trpc' });
export const trpcClient = trpc.createClient({
    links: [chromeLink({ port })],
});
