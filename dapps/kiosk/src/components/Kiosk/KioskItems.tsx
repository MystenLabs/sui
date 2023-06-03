// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  KioskItem,
  KioskListing,
  delist,
  fetchKiosk,
  list,
  place,
  purchaseAndResolvePolicies,
  queryTransferPolicy,
  take,
  testnetEnvironment,
} from '@mysten/kiosk';
import { useRpc } from '../../hooks/useRpc';
import { useEffect, useMemo, useState } from 'react';
import { KioskItem as KioskItemCmp } from './KioskItem';
import { TransactionBlock } from '@mysten/sui.js';
import { ListPrice } from '../Modals/ListPrice';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import {
  getOwnedKiosk,
  getOwnedKioskCap,
  localStorageKeys,
  parseObjectDisplays,
} from '../../utils/utils';
import { useTransactionExecution } from '../../hooks/useTransactionExecution';
import { Loading } from '../Base/Loading';
import { toast } from 'react-hot-toast';
import { useWalletKit } from '@mysten/wallet-kit';
import { useLocation } from 'react-router-dom';

export function KioskItems({
  kioskId,
  address,
}: {
  address?: string;
  kioskId?: string;
}): JSX.Element {
  const provider = useRpc();
  const [loading, setLoading] = useState<boolean>(false);
  const { currentAccount } = useWalletKit();
  const location = useLocation();

  const isKioskPage = location.pathname.startsWith('/kiosk/');

  // we are depending on currentAccount too, as this is what triggers the `getOwnedKioskCap()` function to change
  const kioskOwnerCap = useMemo(() => {
    return getOwnedKioskCap();
  }, [currentAccount?.address]);

  // checks if this is an owned kiosk.
  // We are depending on currentAccount too, as this is what triggers the `getOwnedKioskCap()` function to change
  // using endsWith because we support it with both 0x prefix and without.
  const isOwnedKiosk = useMemo(() => {
    return getOwnedKiosk()?.endsWith(kioskId || '~');
  }, [kioskId, currentAccount?.address]);

  const [modalItem, setModalItem] = useState<OwnedObjectType | null>(null);
  const [kioskItems, setKioskItems] = useState<OwnedObjectType[]>([]);

  const [kioskListings, setKioskListings] =
    useState<Record<string, KioskListing>>();

  const { signAndExecute } = useTransactionExecution();

  useEffect(() => {
    if (!kioskId) return;
    getKioskData();
  }, [kioskId]);

  const getKioskData = async () => {
    if (!kioskId) return;
    setLoading(true);

    try {
      const { data: res } = await fetchKiosk(
        provider,
        kioskId,
        { limit: 1000 },
        {
          withKioskFields: true,
          withListingPrices: true,
        },
      ); // could also add `cursor` for pagination
      // get items.
      const items = await provider.multiGetObjects({
        ids: res.itemIds,
        options: { showDisplay: true, showType: true },
      });

      localStorage.setItem(localStorageKeys.LAST_VISITED_KIOSK_ID, kioskId);

      const displays = parseObjectDisplays(items) || {};
      const ownedItems = res.items.map((item: KioskItem) => {
        return {
          ...item,
          display: displays[item.objectId] || {},
        };
      });
      setKioskItems(ownedItems);
      processKioskListings(res.items.map((x) => x.listing) as KioskListing[]);
    } catch (e) {
      setKioskItems([]);
      toast.error(
        'Something went wrong. Either this is not a valid kiosk address, or the RPC call failed.',
      );
    } finally {
      setLoading(false);
    }
  };

  const processKioskListings = (data: KioskListing[]) => {
    const results: Record<string, KioskListing> = {};

    data
      .filter((x) => !!x)
      .map((x: KioskListing) => {
        results[x.objectId || ''] = x;
      });
    setKioskListings(results);
  };

  const takeFromKiosk = async (item: OwnedObjectType) => {
    if (!item?.objectId || !kioskId || !address || !kioskOwnerCap) return;

    const tx = new TransactionBlock();

    const obj = take(tx, item.type, kioskId, kioskOwnerCap, item.objectId);

    tx.transferObjects([obj], tx.pure(address));

    const success = await signAndExecute({ tx });
    if (success) getKioskData();
  };

  const delistFromKiosk = async (item: OwnedObjectType) => {
    if (!item?.objectId || !kioskId || !address || !kioskOwnerCap) return;
    const tx = new TransactionBlock();

    delist(tx, item.type, kioskId, kioskOwnerCap, item.objectId);

    const success = await signAndExecute({ tx });

    if (success) getKioskData();
  };

  const listToKiosk = async (item: OwnedObjectType, price: string) => {
    if (!kioskId || !kioskOwnerCap) return;

    const tx = new TransactionBlock();

    list(tx, item.type, kioskId, kioskOwnerCap, item.objectId, price);

    const success = await signAndExecute({ tx });

    if (success) {
      getKioskData(); // replace with single kiosk Item search here and replace
      setModalItem(null); // replace modal.
    }
  };

  const purchaseItem = async (item: OwnedObjectType) => {
    const ownedKiosk = getOwnedKiosk();
    const ownedKioskCap = getOwnedKioskCap();

    if (
      !item ||
      !item.listing?.price ||
      !kioskId ||
      !address ||
      !ownedKiosk ||
      !ownedKioskCap
    )
      return;

    const policy = await queryTransferPolicy(provider, item.type);

    const policyId = policy[0]?.id;
    if (!policyId)
      return toast.error(
        `This item doesn't have a Transfer Policy attached so it can't be traded through kiosk.`,
      );

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

  if (loading) return <Loading />;

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
