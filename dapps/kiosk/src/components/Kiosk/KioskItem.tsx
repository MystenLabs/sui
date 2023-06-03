// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { DisplayObject } from '../DisplayObject';
import { useState } from 'react';
// import { Spinner } from "../Spinner";
import { actionWithLoader } from '../../utils/buttons';
import { Button } from '../Base/Button';
import { KioskListing } from '@mysten/kiosk';

export type KioskItemProps = {
  isGuest?: boolean;
  listing?: KioskListing | null;
  takeFn: (item: OwnedObjectType) => void;
  listFn: (item: OwnedObjectType) => void;
  delistFn: (item: OwnedObjectType) => void;
  purchaseFn?: (item: OwnedObjectType, price?: string) => void;
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
}: KioskItemProps): JSX.Element {
  const [loading, setLoading] = useState<boolean>(false);

  if (isGuest)
    return (
      <DisplayObject item={item} listing={listing}>
        <>
          {listing && purchaseFn && (
            <Button
              loading={loading}
              className="btn-outline-primary md:col-span-2"
              onClick={() =>
                actionWithLoader(purchaseFn, { ...item, listing }, setLoading)
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
              loading={loading}
              onClick={() => actionWithLoader(takeFn, item, setLoading)}
            >
              Take from Kiosk
            </Button>

            <Button
              loading={loading}
              className="btn-outline-primary"
              onClick={() => actionWithLoader(listFn, item, setLoading)}
            >
              List for Sale
            </Button>
          </>
        )}
        {listing && !isGuest && (
          <Button
            loading={loading}
            className="btn-outline-primary md:col-span-2"
            onClick={() => actionWithLoader(delistFn, item, setLoading)}
          >
            Delist item
          </Button>
        )}
      </>
    </DisplayObject>
  );
}
