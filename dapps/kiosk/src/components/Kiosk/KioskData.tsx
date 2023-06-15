// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from '@mysten/wallet-kit';
import { Tab } from '@headlessui/react';
import { OwnedObjects } from '../Inventory/OwnedObjects';
import { KioskItems } from './KioskItems';
import { formatAddress } from '@mysten/sui.js';
import { ExplorerLink } from '../Base/ExplorerLink';
import { formatSui, mistToSui } from '../../utils/utils';
import { toast } from 'react-hot-toast';
import { useKioskDetails, useOwnedKiosk } from '../../hooks/kiosk';
import { Loading } from '../Base/Loading';
import { useQueryClient } from '@tanstack/react-query';
import { TANSTACK_KIOSK_DATA_KEY } from '../../utils/constants';
import { Button } from '../Base/Button';
import { useWithdrawMutation } from '../../mutations/kiosk';

export function KioskData() {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk();
	const kioskId = ownedKiosk?.kioskId;

	const { data: kiosk, isLoading } = useKioskDetails(kioskId);

	const queryClient = useQueryClient();

	const withdrawMutation = useWithdrawMutation({
		onSuccess: () => {
			toast.success('Profits withdrawn successfully');
			// invalidate query to refetch kiosk data and update the balance.
			queryClient.invalidateQueries([TANSTACK_KIOSK_DATA_KEY, kioskId]);
		},
	});

	const profits = formatSui(mistToSui(kiosk?.profits));

	if (isLoading) return <Loading />;
	return (
		<div className="container">
			<div className="my-12 ">
				{kiosk && (
					<div className="gap-5 items-center">
						<div>
							Selected Kiosk: {<ExplorerLink text={formatAddress(kiosk.id)} object={kiosk.id} />}
						</div>
						<div className="mt-2">
							Owner (displayed): (
							<ExplorerLink text={formatAddress(kiosk.owner)} address={kiosk.owner} />)
						</div>
						<div className="mt-2">Items Count: {kiosk.itemCount}</div>
						<div className="mt-2">
							Profits: {profits} SUI
							{Number(kiosk.profits) > 0 && (
								<Button
									loading={withdrawMutation.isLoading}
									className=" ease-in-out duration-300 rounded border border-transparent px-4 bg-gray-200 text-xs !py-1 ml-3"
									onClick={() => withdrawMutation.mutate(kiosk)}
								>
									Withdraw all
								</Button>
							)}
						</div>
						<div className="mt-2">UID Exposed: {kiosk.allowExtensions.toString()} </div>
					</div>
				)}
			</div>

			<Tab.Group vertical defaultIndex={0}>
				<Tab.List>
					<Tab className="tab-title">My Kiosk</Tab>
					<Tab className="tab-title">My Wallet</Tab>
				</Tab.List>
				<Tab.Panels>
					<Tab.Panel>
						<p className="pt-6 lg:w-6/12">
							Your kiosk holds the items displayed here. You can perform the following actions:{' '}
							<strong>Take from kiosk</strong>, <strong>List for sale</strong>, and{' '}
							<strong>Remove listing</strong>. You are charged a gas fee for each action you take.
						</p>
						{kioskId && <KioskItems kioskId={kioskId}></KioskItems>}
					</Tab.Panel>
					<Tab.Panel>
						<p className="py-6 lg:w-6/12">
							To add an item to your kiosk, click <strong>Place in kiosk</strong>. To add an item
							and list it for sale, click <strong>List for sale</strong>. You can also add an item
							and then list it after you add it. You are changed a gas fee each time you place,
							list, or remove an item.
						</p>
						{currentAccount && <OwnedObjects address={currentAccount.address}></OwnedObjects>}
					</Tab.Panel>
				</Tab.Panels>
			</Tab.Group>
		</div>
	);
}
