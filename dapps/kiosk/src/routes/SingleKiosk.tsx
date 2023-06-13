// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';
import { useEffect, useState } from 'react';
import { KioskItems } from '../components/Kiosk/KioskItems';
import { Loading } from '../components/Base/Loading';
import { useOwnedKiosk } from '../hooks/kiosk';
import { KioskSelector } from '../components/Kiosk/KioskSelector';
import { KioskOwnerCap } from '@mysten/kiosk';

export default function SingleKiosk() {
  const { id } = useParams();

  // tries to find an owned kiosk for the supplied id.
  // will fail if it's a direct kioskId and pass it down directly.
  const { data: ownedKiosk, isLoading } = useOwnedKiosk(id);

  const [selected, setSelected] = useState<KioskOwnerCap | undefined>();

  // show kiosk selector in the following conditions:
  // 1. It's an address lookup.
  // 2. The address has more than 1 kiosks.
  const showKioskSelector =
    ownedKiosk?.caps && ownedKiosk.caps.length > 1 && selected;

  useEffect(() => {
    if (isLoading) return;
    setSelected(ownedKiosk?.caps[0]);
  }, [isLoading, ownedKiosk?.caps, setSelected]);

  if (isLoading) return <Loading />;

  return (
    <div className="container">
      {showKioskSelector && (
        <KioskSelector
          caps={ownedKiosk.caps}
          selected={selected}
          setSelected={setSelected}
        />
      )}
      <KioskItems kioskId={selected?.kioskId || id}></KioskItems>
    </div>
  );
}
