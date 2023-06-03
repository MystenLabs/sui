// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { ModalBase } from './Base';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { DisplayObjectThumbnail } from '../DisplayObjectThumbnail';
import { Button } from '../Base/Button';

export interface ListPriceProps {
  item: OwnedObjectType;
  onSubmit: (item: OwnedObjectType, price: string) => void;
  closeModal: () => void;
}
export function ListPrice({
  item,
  onSubmit,
  closeModal,
}: ListPriceProps): JSX.Element {
  const [price, setPrice] = useState<string>('');
  const [loading, setLoading] = useState<boolean>(false);

  const list = async () => {
    setLoading(true);
    if (onSubmit) await onSubmit(item, price);
    setLoading(false);
  };
  return (
    <ModalBase
      isOpen
      closeModal={closeModal}
      title="Select the listing price in SUI"
    >
      <>
        <div>
          <DisplayObjectThumbnail item={item}></DisplayObjectThumbnail>
        </div>
        <div>
          <label>Listing price</label>
          <input
            value={price}
            className="content"
            placeholder="The amount in SUI"
            onChange={(e) => setPrice(e.target.value)}
          ></input>
        </div>

        <div className="mt-6">
          <Button
            loading={loading}
            className="btn-primary w-full"
            onClick={list}
          >
            List Item
          </Button>
        </div>
      </>
    </ModalBase>
  );
}
