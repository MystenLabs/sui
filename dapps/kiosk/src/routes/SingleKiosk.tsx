// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { Loading } from '../components/Base/Loading';
import { KioskItems } from '../components/Kiosk/KioskItems';
import { KioskSelector } from '../components/Kiosk/KioskSelector';
import { useOwnedKiosk } from '../hooks/kiosk';
import { useKioskSelector } from '../hooks/useKioskSelector';

export default function SingleKiosk() {
	const { id } = useParams();

	// tries to find an owned kiosk for the supplied id.
	// will fail if it's a direct kioskId and pass it down directly.
	const { data: ownedKiosk, isPending } = useOwnedKiosk(id);
	const { selected, setSelected, showKioskSelector } = useKioskSelector(id);

	if (isPending) return <Loading />;

	return (
		<div className="container">
			{showKioskSelector && selected && ownedKiosk && (
				<KioskSelector caps={ownedKiosk.caps} selected={selected} setSelected={setSelected} />
			)}
			<KioskItems kioskId={selected?.kioskId || id}></KioskItems>
		</div>
	);
}
