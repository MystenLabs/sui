// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { OwnedObject } from './OwnedObject';
import { KioskItem } from '@mysten/kiosk';
import { ListPrice } from '../Modals/ListPrice';
import { Loading } from '../Base/Loading';
import { useOwnedObjects } from '../../hooks/useOwnedObjects';
import { toast } from 'react-hot-toast';
import { ObjectId } from '@mysten/sui.js';

export type OwnedObjectType = KioskItem & {
	display: Record<string, string>;
};

export function OwnedObjects({ address, kioskId }: { address: string; kioskId: ObjectId }) {
	const [modalItem, setModalItem] = useState<OwnedObjectType | null>(null);

	const {
		data: ownedObjects,
		isLoading,
		refetch: getOwnedObjects,
	} = useOwnedObjects({
		address,
	});

	if (isLoading) return <Loading />;

	return (
		<div className="grid grid-cols-2 lg:grid-cols-4 gap-5 pt-12">
			{/* Only shows item with an image_url to make it easier to understand the flows. */}
			{ownedObjects
				?.filter((x) => !!x.display && !!x.display.image_url)
				.map((item) => (
					<OwnedObject
						kioskId={kioskId}
						key={item.objectId}
						object={item}
						onListSuccess={() => {
							toast.success('Item listed successfully.');
							getOwnedObjects();
						}}
						listFn={(selectedItem: OwnedObjectType) => setModalItem(selectedItem)}
					/>
				))}

			{modalItem && (
				<ListPrice
					kioskId={kioskId}
					item={modalItem}
					listAndPlace
					onSuccess={() => {
						toast.success('Item listed for sale successfully!');
						getOwnedObjects();
						setModalItem(null); // replace modal.
					}}
					closeModal={() => setModalItem(null)}
				/>
			)}
		</div>
	);
}
