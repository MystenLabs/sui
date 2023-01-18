// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createTRPCReact } from '@trpc/react-query';

import { backgroundLink } from './link';

import type { AppRouter } from '_src/background/trpc';

export const trpc = createTRPCReact<AppRouter>();

export const trpcClient = trpc.createClient({
    links: [backgroundLink()],
});
