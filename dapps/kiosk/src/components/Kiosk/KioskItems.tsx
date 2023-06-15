// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { KioskItem as KioskItemCmp } from './KioskItem';
import { ListPrice } from '../Modals/ListPrice';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { Loading } from '../Base/Loading';
import { toast } from 'react-hot-toast';
import { useLocation, useNavigate } from 'react-router-dom';
import { useKiosk, useOwnedKiosk } from '../../hooks/kiosk';
import { KioskNotFound } from './KioskNotFound';

export function KioskItems({ kioskId }: { kioskId?: string }) {
	const location = useLocation();
	const isKioskPage = location.pathname.startsWith('/kiosk/');

	const { data: walletKiosk } = useOwnedKiosk();
	const ownedKiosk = walletKiosk?.kioskId;

	// checks if this is an owned kiosk.
	// We are depending on currentAccount too, as this is what triggers the `getOwnedKioskCap()` function to change
	// using endsWith because we support it with both 0x prefix and without.
	const isOwnedKiosk = () => {
		return ownedKiosk?.endsWith(kioskId || '~');
	};

	const [modalItem, setModalItem] = useState<OwnedObjectType | null>(null);

	const { data: kioskData, isLoading, isError, refetch: getKioskData } = useKiosk(kioskId);

	const navigate = useNavigate();

	useEffect(() => {
		if (!isError) return;
		toast.error(
			'The requested kiosk was not found. You either supplied a wrong kiosk Id or the RPC call failed.',
		);
	}, [navigate, isError]);

	const kioskItems = kioskData?.items || [];
	const kioskListings = kioskData?.listings || {};

	if (isError && isKioskPage) return <KioskNotFound />;

	if (isLoading) return <Loading />;

	if (kioskItems.length === 0)
		return <div className="py-12">The kiosk you are viewing is empty!</div>;

	return (
		<div className="mt-12">
			{
				// We're hiding this when we've clicked "view kiosk" for our own kiosk.
				isOwnedKiosk() && isKioskPage && (
					<div className="bg-yellow-300 text-black rounded px-3 py-2 mb-6">
						You're viewing your own kiosk
					</div>
				)
			}
			<div className="grid sm:grid-cols-2 xl:grid-cols-4 gap-5">
				{kioskId &&
					kioskItems.map((item: OwnedObjectType) => (
						<KioskItemCmp
							key={item.objectId}
							kioskId={kioskId}
							item={item}
							isGuest={!isOwnedKiosk()}
							onSuccess={() => {
								getKioskData();
							}}
							listing={kioskListings && kioskListings[item.objectId]}
							listFn={(item: OwnedObjectType) => setModalItem(item)}
						/>
					))}
				{modalItem && (
					<ListPrice
						item={modalItem}
						onSuccess={() => {
							toast.success('Item listed successfully.');
							getKioskData(); // replace with single kiosk Item search here and replace
							setModalItem(null); // replace modal.
						}}
						closeModal={() => setModalItem(null)}
					/>
				)}
			</div>
		</div>
	);
}
