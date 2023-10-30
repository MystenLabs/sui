// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KioskOwnerCap } from '@mysten/kiosk';
import { useEffect, useState } from 'react';

import { useOwnedKiosk } from './kiosk';

export function useKioskSelector(address: string | undefined) {
	const [selected, setSelected] = useState<KioskOwnerCap | undefined>();

	// tries to find an owned kiosk for the supplied id.
	// will fail if it's a direct kioskId and pass it down directly.
	const { data: ownedKiosk, isPending } = useOwnedKiosk(address);

	// show kiosk selector in the following conditions:
	// 1. It's an address lookup.
	// 2. The address has more than 1 kiosks.
	const showKioskSelector = ownedKiosk?.caps && ownedKiosk.caps.length > 1 && selected;

	useEffect(() => {
		// reset when kiosk caps change,
		// (on logout / or if a cap is transferred away).
		if (!ownedKiosk?.caps.find((x) => x.objectId === selected?.objectId))
			setSelected(ownedKiosk?.caps[0]);

		if (isPending || selected) return;
		setSelected(ownedKiosk?.caps[0]);
	}, [isPending, selected, ownedKiosk?.caps, setSelected]);

	return {
		selected,
		setSelected,
		showKioskSelector,
	};
}
