// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { QueryClient } from '@tanstack/react-query';

export const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			retry: false,
			refetchOnMount: false,
			refetchInterval: false,
			refetchOnWindowFocus: false,
			refetchIntervalInBackground: false,
		},
	},
});
