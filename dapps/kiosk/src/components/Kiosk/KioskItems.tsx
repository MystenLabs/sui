// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount } from '@mysten/dapp-kit';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import { useEffect, useState } from 'react';
import { toast } from 'react-hot-toast';
import { useLocation, useNavigate } from 'react-router-dom';

import { useKiosk, useOwnedKiosk } from '../../hooks/kiosk';
import { Loading } from '../Base/Loading';
import { OwnedObjectType } from '../Inventory/OwnedObjects';
import { ListPrice } from '../Modals/ListPrice';
import { KioskItem as KioskItemCmp } from './KioskItem';
import { KioskNotFound } from './KioskNotFound';

export function KioskItems({ kioskId }: { kioskId?: string }) {
	const location = useLocation();
	const isKioskPage = location.pathname.startsWith('/kiosk/');
	const currentAccount = useCurrentAccount();

	const { data: walletKiosk } = useOwnedKiosk(currentAccount?.address);

	// checks if this is an owned kiosk.
	// We are depending on currentAccount too, as this is what triggers the `getOwnedKioskCap()` function to change
	// using endsWith because we support it with both 0x prefix and without.
	const isOwnedKiosk = () => {
		return walletKiosk?.caps?.find(
			(x) => kioskId && normalizeSuiAddress(x.kioskId).endsWith(kioskId),
		);
	};

	const [modalItem, setModalItem] = useState<OwnedObjectType | null>(null);

	const { data: kioskData, isPending, isError, refetch: getKioskData } = useKiosk(kioskId);

	const navigate = useNavigate();

	useEffect(() => {
		if (!isError) return;
		toast.error(
			'The requested kiosk was not found. You either supplied a wrong kiosk Id or the RPC call failed.',
		);
	}, [navigate, isError]);

	const kioskItems = kioskData?.items || [];
	const kioskListings = kioskData?.listings || {};

	if (!kioskId) return <div className="py-12">Supply a kiosk ID to continue.</div>;

	if (isError && isKioskPage) return <KioskNotFound />;

	if (isPending) return <Loading />;

	if (!kioskId || kioskItems.length === 0)
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
				{kioskItems.map((item: OwnedObjectType) => (
					<KioskItemCmp
						key={item.objectId}
						kioskId={kioskId}
						item={item}
						isGuest={!isOwnedKiosk()}
						hasKiosk={!!walletKiosk?.kioskId}
						onSuccess={() => {
							getKioskData();
						}}
						listing={kioskListings && kioskListings[item.objectId]}
						listFn={(item: OwnedObjectType) => setModalItem(item)}
					/>
				))}
				{modalItem && (
					<ListPrice
						kioskId={kioskId}
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
