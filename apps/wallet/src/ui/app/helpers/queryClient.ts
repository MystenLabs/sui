// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { QueryClient } from '@tanstack/react-query';

export const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            // Only retry once by default:
            retry: 1,
            // Default stale time to 30 seconds, which seems like a sensible tradeoff between network requests and stale data.
            staleTime: 30 * 1000,
            // Disable automatic interval fetching
            refetchInterval: 0,
            refetchIntervalInBackground: false,
        },
    },
});
