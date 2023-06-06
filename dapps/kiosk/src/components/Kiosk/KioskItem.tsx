// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { DisplayObject } from '../DisplayObject';
// import { Spinner } from "../Spinner";
import { Button } from '../Base/Button';
import { KioskListing } from '@mysten/kiosk';
import { KioskFnType, useKioskMutationFn } from '../../hooks/kiosk';

export type KioskItemProps = {
  isGuest?: boolean;
  listing?: KioskListing | null;
  takeFn: KioskFnType;
  listFn: KioskFnType;
  delistFn: KioskFnType;
  purchaseFn?: KioskFnType;
  item: OwnedObjectType;
};

export function KioskItem({
  item,
  listing = null,
  isGuest = false,
  purchaseFn,
  takeFn,
  listFn,
  delistFn,
}: KioskItemProps) {
  const mutation = useKioskMutationFn();

  if (isGuest)
    return (
      <DisplayObject item={item} listing={listing}>
        <>
          {listing && purchaseFn && (
            <Button
              loading={mutation.isLoading}
              className="border-gray-400 bg-transparent hover:bg-primary hover:text-white md:col-span-2"
              onClick={() =>
                mutation.mutate({
                  fn: purchaseFn,
                  object: { ...item, listing },
                })
              }
            >
              Purchase
            </Button>
          )}
        </>
      </DisplayObject>
    );
  return (
    <DisplayObject item={item} listing={listing}>
      <>
        {!listing && !isGuest && (
          <>
            <Button
              className="border-transparent py-2 px-4 bg-gray-200"
              loading={mutation.isLoading}
              onClick={() => mutation.mutate({ fn: takeFn, object: item })}
            >
              Take from Kiosk
            </Button>

            <Button
              loading={mutation.isLoading}
              className="border-gray-400 bg-transparent hover:bg-primary hover:text-white"
              onClick={() => mutation.mutate({ fn: listFn, object: item })}
            >
              List for Sale
            </Button>
          </>
        )}
        {listing && !isGuest && (
          <Button
            loading={mutation.isLoading}
            className="border-gray-400 bg-transparent hover:bg-primary hover:text-white md:col-span-2"
            onClick={() => mutation.mutate({ fn: delistFn, object: item })}
          >
            Delist item
          </Button>
        )}
      </>
    </DisplayObject>
  );
}
