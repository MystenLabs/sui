// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';

import { useOwnedKiosk } from '../hooks/kiosk';
import { OwnedObjectType } from '../components/Inventory/OwnedObjects';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { Kiosk, KioskTransaction } from '@mysten/kiosk';
import { useTransactionExecution } from '../hooks/useTransactionExecution';
import { useWalletKit } from '@mysten/wallet-kit';
// import { useRpc } from '../context/RpcClientContext';
import { toast } from 'react-hot-toast';
import { findActiveCap } from '../utils/utils';
import { useKioskClient } from '../context/KioskClientContext';

type MutationParams = {
	onSuccess?: () => void;
	onError?: (e: Error) => void;
};

const defaultOnError = (e: Error) => {
	if (typeof e === 'string') toast.error(e);
	else toast.error(e?.message);
};

/**
 * Create a new kiosk.
 */
export function useCreateKioskMutation({ onSuccess, onError }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: () => {
			if (!currentAccount?.address) throw new Error('You need to connect your wallet!');
			const txb = new TransactionBlock();
			const kioskTx = new KioskTransaction({ txb, kioskClient });
			kioskTx.createAndShare(currentAccount?.address);
			return signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}

/**
 * Place & List or List for sale in kiosk.
 */
export function usePlaceAndListMutation({ onSuccess, onError }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk(currentAccount?.address);
	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: async ({
			item,
			price,
			shouldPlace,
			kioskId,
		}: {
			item: OwnedObjectType;
			price: string;
			shouldPlace?: boolean;
			kioskId: string;
		}) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			const txb = new TransactionBlock();

			const kioskTx = new KioskTransaction({ kioskClient, txb, cap });

			if (shouldPlace) {
				kioskTx.placeAndList({
					item: item.objectId,
					itemType: item.type,
					price,
				});
			} else {
				kioskTx.list({
					itemId: item.objectId,
					itemType: item.type,
					price,
				});
			}

			kioskTx.wrap();

			return signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}

/**
 * Mutation to place an item in the kiosk.
 */
export function usePlaceMutation({ onSuccess, onError }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk(currentAccount?.address);
	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: async ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			const txb = new TransactionBlock();
			const kioskTx = new KioskTransaction({ txb, kioskClient, cap });
			kioskTx.place({ itemType: item.type, item: item.objectId });
			kioskTx.wrap();

			return signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}

/**
 * Withdraw profits from kiosk
 */
export function useWithdrawMutation({ onError, onSuccess }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk(currentAccount?.address);

	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: async ({ id, profits }: Kiosk) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, id);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');
			const txb = new TransactionBlock();

			const kioskTx = new KioskTransaction({ txb, kioskClient, cap });
			kioskTx.withdraw(currentAccount.address, profits);
			kioskTx.wrap();

			return signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}

/**
 * Mutation to take an item from the kiosk.
 */
export function useTakeMutation({ onSuccess, onError }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk(currentAccount?.address);
	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: async ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			if (!item?.objectId) throw new Error('Missing item.');
			const txb = new TransactionBlock();
			const kioskTx = new KioskTransaction({ txb, kioskClient, cap });

			kioskTx.transfer({
				itemType: item.type,
				itemId: item.objectId,
				address: currentAccount.address,
			});

			kioskTx.wrap();

			return signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}

/**
 * Mutation to delist an item.
 */
export function useDelistMutation({ onSuccess, onError }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk(currentAccount?.address);
	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: async ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			if (!item?.objectId) throw new Error('Missing item.');

			const txb = new TransactionBlock();

			const kioskTx = new KioskTransaction({ txb, kioskClient, cap });

			kioskTx.delist({
				itemType: item.type,
				itemId: item.objectId,
			});

			kioskTx.wrap();

			return signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}

/**
 * Mutation to delist an item.
 */
export function usePurchaseItemMutation({ onSuccess, onError }: MutationParams) {
	const { currentAccount } = useWalletKit();
	const { data: ownedKiosk } = useOwnedKiosk(currentAccount?.address);
	const { signAndExecute } = useTransactionExecution();
	const kioskClient = useKioskClient();

	return useMutation({
		mutationFn: async ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			if (
				!item ||
				!item.listing?.price ||
				!kioskId ||
				!currentAccount?.address ||
				!ownedKiosk?.kioskId ||
				!ownedKiosk.kioskCap
			)
				throw new Error('Missing parameters');

			const cap = findActiveCap(ownedKiosk?.caps, ownedKiosk.kioskId);
			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			const txb = new TransactionBlock();
			const kioskTx = new KioskTransaction({ txb, kioskClient, cap });

			await kioskTx.purchaseAndResolve({
				itemType: item.type,
				itemId: item.objectId,
				sellerKiosk: kioskId,
				price: item.listing!.price!,
			});

			kioskTx.wrap();
			return await signAndExecute({ tx: txb });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}
