// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ampli } from '_src/shared/analytics/ampli';
import { useEffect } from 'react';

import { useCheckBlocklist } from '../../hooks/useDomainBlocklist';

export function useShowScamWarning({ hostname }: { hostname?: string }) {
	const { data, isPending, isError } = useCheckBlocklist(hostname);

	useEffect(() => {
		if (data?.block && hostname) {
			ampli.interactedWithMaliciousDomain({ hostname });
		}
	}, [data, hostname]);

	return {
		isOpen: !!data?.block,
		isPending: isPending,
		isError,
	};
}
