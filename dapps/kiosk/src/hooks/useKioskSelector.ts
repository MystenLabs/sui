import { KioskOwnerCap } from '@mysten/kiosk';
import { SuiAddress } from '@mysten/sui.js';
import { useEffect, useState } from 'react';
import { useOwnedKiosk } from './kiosk';

export function useKioskSelector(kioskId: SuiAddress | undefined) {
  const [selected, setSelected] = useState<KioskOwnerCap | undefined>();

  // tries to find an owned kiosk for the supplied id.
  // will fail if it's a direct kioskId and pass it down directly.
  const { data: ownedKiosk, isLoading } = useOwnedKiosk(kioskId);

  // show kiosk selector in the following conditions:
  // 1. It's an address lookup.
  // 2. The address has more than 1 kiosks.
  const showKioskSelector =
    ownedKiosk?.caps && ownedKiosk.caps.length > 1 && selected;

  useEffect(() => {
    if (isLoading || selected) return;
    setSelected(ownedKiosk?.caps[0]);
  }, [isLoading, selected, ownedKiosk?.caps, setSelected]);

  return {
    selected,
    setSelected,
    showKioskSelector,
  };
}
