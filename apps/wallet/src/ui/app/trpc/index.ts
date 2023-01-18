import { createTRPCReact } from '@trpc/react-query';

import { backgroundLink } from './link';

import type { AppRouter } from '_src/background/trpc';

export const trpc = createTRPCReact<AppRouter>();

export const trpcClient = trpc.createClient({
    links: [backgroundLink()],
});
