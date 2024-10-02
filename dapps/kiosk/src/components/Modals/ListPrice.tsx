// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MIST_PER_SUI } from '@mysten/sui/utils';
import { useState } from 'react';

import { usePlaceAndListMutation } from '../../mutations/kiosk';
import { Button } from '../Base/Button';
import { DisplayObjectThumbnail } from '../DisplayObjectThumbnail';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { ModalBase } from './Base';

export interface ListPriceProps {
	item: OwnedObjectType;
	onSuccess: () => void;
	closeModal: () => void;
	listAndPlace?: boolean;
	kioskId: string;
}
export function ListPrice({ item, onSuccess, closeModal, listAndPlace, kioskId }: ListPriceProps) {
	const [price, setPrice] = useState<string>('');

	const placeAndListToKioskMutation = usePlaceAndListMutation({
		onSuccess: onSuccess,
	});

	return (
		<ModalBase isOpen closeModal={closeModal} title="Select the listing price">
			<>
				<div>
					<DisplayObjectThumbnail item={item}></DisplayObjectThumbnail>
				</div>
				<div>
					<label className="font-medium mb-1 block text-sm">Listing price (in SUI)</label>
					<input
						type="number"
						min="0"
						value={price}
						className="block w-full rounded border border-primary bg-white p-2.5 text-sm outline-primary focus:border-gray-500"
						placeholder="The amount in SUI"
						onChange={(e) => setPrice(e.target.value)}
					></input>
				</div>

				<div className="mt-6">
					<Button
						loading={placeAndListToKioskMutation.isPending}
						className="ease-in-out duration-300 rounded py-2 px-4 bg-primary text-white hover:opacity-70 w-full"
						onClick={() =>
							placeAndListToKioskMutation.mutate({
								item,
								price: (Number(price) * Number(MIST_PER_SUI)).toString(),
								shouldPlace: listAndPlace,
								kioskId,
							})
						}
					>
						List Item
					</Button>
				</div>
			</>
		</ModalBase>
	);
}
