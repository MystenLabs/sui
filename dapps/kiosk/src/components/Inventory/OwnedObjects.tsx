// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { TransactionBlock } from '@mysten/sui.js';
import { OwnedObject } from './OwnedObject';
import { useTransactionExecution } from '../../hooks/useTransactionExecution';
import { KioskItem, place, placeAndList } from '@mysten/kiosk';
import { ListPrice } from '../Modals/ListPrice';
import { Loading } from '../Base/Loading';
import { useOwnedKiosk } from '../../hooks/kiosk';
import { useOwnedObjects } from '../../hooks/useOwnedObjects';

export type OwnedObjectType = KioskItem & {
  display: Record<string, string>;
};

export function OwnedObjects({ address }: { address: string }): JSX.Element {
  const { data: ownedKiosk } = useOwnedKiosk();
  const kioskId = ownedKiosk?.kioskId;
  const kioskOwnerCap = ownedKiosk?.kioskCap;

  const [modalItem, setModalItem] = useState<OwnedObjectType | null>(null);
  const { signAndExecute } = useTransactionExecution();

  const {
    data: ownedObjects,
    isLoading,
    refetch: getOwnedObjects,
  } = useOwnedObjects({
    address,
  });

  const placeToKiosk = async (item: OwnedObjectType) => {
    if (!kioskId || !kioskOwnerCap) return;

    const tx = new TransactionBlock();
    place(tx, item.type, kioskId, kioskOwnerCap, item.objectId);
    const success = await signAndExecute({ tx });
    if (success) getOwnedObjects();
  };

  const placeAndListToKiosk = async (item: OwnedObjectType, price: string) => {
    if (!kioskId || !kioskOwnerCap) return;
    const tx = new TransactionBlock();
    placeAndList(tx, item.type, kioskId, kioskOwnerCap, item.objectId, price);
    const success = await signAndExecute({ tx });
    if (success) {
      getOwnedObjects();
      setModalItem(null); // replace modal.
    }
  };

  if (isLoading) return <Loading />;

  return (
    <div className="grid grid-cols-2 lg:grid-cols-4 gap-5">
      {ownedObjects?.map((item) => (
        <OwnedObject
          key={item.objectId}
          object={item}
          placeFn={placeToKiosk}
          listFn={(selectedItem: OwnedObjectType) => setModalItem(selectedItem)}
        />
      ))}

      {modalItem && (
        <ListPrice
          item={modalItem}
          onSubmit={placeAndListToKiosk}
          closeModal={() => setModalItem(null)}
        />
      )}
    </div>
  );
}
