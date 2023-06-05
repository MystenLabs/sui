// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  delist,
  list,
  place,
  purchaseAndResolvePolicies,
  queryTransferPolicy,
  take,
  testnetEnvironment,
} from '@mysten/kiosk';
import { useMemo, useState } from 'react';
import { KioskItem as KioskItemCmp } from './KioskItem';
import { TransactionBlock } from '@mysten/sui.js';
import { ListPrice } from '../Modals/ListPrice';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { useTransactionExecution } from '../../hooks/useTransactionExecution';
import { Loading } from '../Base/Loading';
import { toast } from 'react-hot-toast';
import { useWalletKit } from '@mysten/wallet-kit';
import { useLocation, useNavigate } from 'react-router-dom';
import { useRpc } from '../../context/RpcClientContext';
import { useKiosk, useOwnedKiosk } from '../../hooks/kiosk';

export function KioskItems({ kioskId }: { kioskId?: string }): JSX.Element {
  const provider = useRpc();
  const { currentAccount } = useWalletKit();
  const location = useLocation();

  const { data: walletKiosk } = useOwnedKiosk();
  const ownedKioskCap = walletKiosk?.kioskCap;
  const ownedKiosk = walletKiosk?.kioskId;

  const isKioskPage = location.pathname.startsWith('/kiosk/');

  // checks if this is an owned kiosk.
  // We are depending on currentAccount too, as this is what triggers the `getOwnedKioskCap()` function to change
  // using endsWith because we support it with both 0x prefix and without.
  const isOwnedKiosk = useMemo(() => {
    return ownedKiosk?.endsWith(kioskId || '~');
  }, [ownedKiosk, kioskId]);

  const [modalItem, setModalItem] = useState<OwnedObjectType | null>(null);

  const {
    data: kioskData,
    isLoading,
    isError,
    refetch: getKioskData,
  } = useKiosk(kioskId);

  const navigate = useNavigate();
  if (isError) {
    toast.error(
      'The requested kiosk was not found. You either supplied a wrong kiosk Id or the RPC call failed.',
    );
    navigate('/');
  }

  const kioskItems = kioskData?.items || [];
  const kioskListings = kioskData?.listings || {};

  const { signAndExecute } = useTransactionExecution();

  const takeFromKiosk = async (item: OwnedObjectType) => {
    if (
      !item?.objectId ||
      !kioskId ||
      !currentAccount?.address ||
      !ownedKioskCap
    )
      return;

    const tx = new TransactionBlock();

    const obj = take(tx, item.type, kioskId, ownedKioskCap, item.objectId);

    tx.transferObjects([obj], tx.pure(currentAccount?.address));

    const success = await signAndExecute({ tx });
    if (success) getKioskData();
  };

  const delistFromKiosk = async (item: OwnedObjectType) => {
    if (
      !item?.objectId ||
      !kioskId ||
      !currentAccount?.address ||
      !ownedKioskCap
    )
      return;
    const tx = new TransactionBlock();

    delist(tx, item.type, kioskId, ownedKioskCap, item.objectId);

    const success = await signAndExecute({ tx });

    if (success) getKioskData();
  };

  const listToKiosk = async (item: OwnedObjectType, price: string) => {
    if (!kioskId || !ownedKioskCap) return;

    const tx = new TransactionBlock();

    list(tx, item.type, kioskId, ownedKioskCap, item.objectId, price);

    const success = await signAndExecute({ tx });

    if (success) {
      getKioskData(); // replace with single kiosk Item search here and replace
      setModalItem(null); // replace modal.
    }
  };

  const purchaseItem = async (item: OwnedObjectType): Promise<void> => {
    if (
      !item ||
      !item.listing?.price ||
      !kioskId ||
      !currentAccount?.address ||
      !ownedKiosk ||
      !ownedKioskCap
    )
      return;

    const policy = await queryTransferPolicy(provider, item.type);

    const policyId = policy[0]?.id;
    if (!policyId) {
      toast.error(
        `This item doesn't have a Transfer Policy attached so it can't be traded through kiosk.`,
      );
      return;
    }

    const tx = new TransactionBlock();

    const environment = testnetEnvironment;

    try {
      const result = purchaseAndResolvePolicies(
        tx,
        item.type,
        item.listing.price,
        kioskId,
        item.objectId,
        policy[0],
        environment,
        {
          ownedKiosk,
          ownedKioskCap,
        },
      );

      if (result.canTransfer)
        place(tx, item.type, ownedKiosk, ownedKioskCap, result.item);

      const success = await signAndExecute({ tx });

      if (success) getKioskData();
    } catch (e: any) {
      toast.error(e?.message);
    }
  };

  if (isLoading) return <Loading />;

  if (kioskItems.length === 0)
    return <div className="py-12">The kiosk you are viewing is empty!</div>;

  return (
    <div className="mt-12">
      {
        // We're hiding this when we've clicked "view kiosk" for our own kiosk.
        isOwnedKiosk && isKioskPage && (
          <div className="bg-yellow-300 text-black rounded px-3 py-2 mb-6">
            You're viewing your own kiosk
          </div>
        )
      }
      <div className="grid sm:grid-cols-2 xl:grid-cols-4 gap-5">
        {kioskItems.map((item: OwnedObjectType) => (
          <KioskItemCmp
            key={item.objectId}
            item={item}
            isGuest={!isOwnedKiosk}
            listing={kioskListings && kioskListings[item.objectId]}
            takeFn={takeFromKiosk}
            //@ts-ignore
            listFn={(item: OwnedObjectType) => setModalItem(item)}
            delistFn={(item: OwnedObjectType) => delistFromKiosk(item)}
            purchaseFn={purchaseItem}
          />
        ))}
        {modalItem && (
          <ListPrice
            item={modalItem}
            onSubmit={listToKiosk}
            closeModal={() => setModalItem(null)}
          />
        )}
      </div>
    </div>
  );
}
