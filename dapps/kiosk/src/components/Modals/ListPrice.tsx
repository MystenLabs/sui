// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { ModalBase } from './Base';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { DisplayObjectThumbnail } from '../DisplayObjectThumbnail';
import { Button } from '../Base/Button';
import { MIST_PER_SUI } from '@mysten/sui.js';

export interface ListPriceProps {
  item: OwnedObjectType;
  onSubmit: (item: OwnedObjectType, price: string) => void;
  closeModal: () => void;
}
export function ListPrice({ item, onSubmit, closeModal }: ListPriceProps) {
  const [price, setPrice] = useState<string>('');
  const [loading, setLoading] = useState<boolean>(false);

  const list = async () => {
    setLoading(true);
    if (onSubmit) await onSubmit(item, price);
    setLoading(false);
  };
  return (
    <ModalBase isOpen closeModal={closeModal} title="Select the listing price">
      <>
        <div>
          <DisplayObjectThumbnail item={item}></DisplayObjectThumbnail>
        </div>
        <div>
          <label className="font-medium mb-1 block text-sm">
            Listing price (in MIST) ({Number(price) / Number(MIST_PER_SUI)} SUI)
          </label>
          <input
            value={price}
            className="block w-full rounded border border-primary bg-white p-2.5 text-sm outline-primary focus:border-gray-500"
            placeholder="The amount in SUI"
            onChange={(e) => setPrice(e.target.value)}
          ></input>
        </div>

        <div className="mt-6">
          <Button
            loading={loading}
            className="ease-in-out duration-300 rounded py-2 px-4 bg-primary text-white hover:opacity-70 w-full"
            onClick={list}
          >
            List Item
          </Button>
        </div>
      </>
    </ModalBase>
  );
}
