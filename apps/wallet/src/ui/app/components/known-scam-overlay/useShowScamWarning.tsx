// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ampli } from '_src/shared/analytics/ampli';
import { useEffect, useState } from 'react';

import { useCheckBlocklist } from '../../hooks/useDomainBlocklist';

export function useShowScamWarning({ hostname }: { hostname?: string }) {
	const [userBypassed, setUserBypassed] = useState(false);
	const { data } = useCheckBlocklist(hostname);
	const bypass = () => setUserBypassed(true);

	useEffect(() => {
		if (data?.block && hostname) {
			ampli.interactedWithMaliciousDomain({ hostname });
		}
	}, [data, hostname]);

	return {
		isOpen: !!data?.block && !userBypassed,
		userBypassed,
		bypass,
	};
}
