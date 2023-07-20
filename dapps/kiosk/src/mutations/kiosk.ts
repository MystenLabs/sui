// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';

import { useOwnedKiosk } from '../hooks/kiosk';
import { OwnedObjectType } from '../components/Inventory/OwnedObjects';
import { ObjectId } from '@mysten/sui.js';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import {
	Kiosk,
	createKioskAndShare,
	delist,
	list,
	place,
	placeAndList,
	purchaseAndResolvePolicies,
	queryTransferPolicy,
	take,
	testnetEnvironment,
	withdrawFromKiosk,
} from '@mysten/kiosk';
import { useTransactionExecution } from '../hooks/useTransactionExecution';
import { useWalletKit } from '@mysten/wallet-kit';
import { useRpc } from '../context/RpcClientContext';
import { toast } from 'react-hot-toast';
import { findActiveCap } from '../utils/utils';

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

	return useMutation({
		mutationFn: () => {
			if (!currentAccount?.address) throw new Error('You need to connect your wallet!');
			const tx = new TransactionBlock();
			const kiosk_cap = createKioskAndShare(tx);
			tx.transferObjects([kiosk_cap], tx.pure(currentAccount.address, 'address'));
			return signAndExecute({ tx });
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

	return useMutation({
		mutationFn: ({
			item,
			price,
			shouldPlace,
			kioskId,
		}: {
			item: OwnedObjectType;
			price: string;
			shouldPlace?: boolean;
			kioskId: ObjectId;
		}) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			const tx = new TransactionBlock();

			if (shouldPlace) placeAndList(tx, item.type, cap.kioskId, cap.objectId, item.objectId, price);
			else list(tx, item.type, cap.kioskId, cap.objectId, item.objectId, price);

			return signAndExecute({ tx });
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

	return useMutation({
		mutationFn: ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			const tx = new TransactionBlock();
			place(tx, item.type, cap.kioskId, cap.objectId, item.objectId);

			return signAndExecute({ tx });
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

	return useMutation({
		mutationFn: (kiosk: Kiosk) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kiosk.id);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			const tx = new TransactionBlock();
			const coin = withdrawFromKiosk(tx, cap.kioskId, cap.objectId, kiosk.profits);

			tx.transferObjects([coin], tx.pure(currentAccount.address, 'address'));

			return signAndExecute({ tx });
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

	return useMutation({
		mutationFn: ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			if (!item?.objectId) throw new Error('Missing item.');

			const tx = new TransactionBlock();

			const obj = take(tx, item.type, cap.kioskId, cap.objectId, item.objectId);

			tx.transferObjects([obj], tx.pure(currentAccount?.address));

			return signAndExecute({ tx });
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

	return useMutation({
		mutationFn: ({ item, kioskId }: { item: OwnedObjectType; kioskId: string }) => {
			// find active kiosk cap.
			const cap = findActiveCap(ownedKiosk?.caps, kioskId);

			if (!cap || !currentAccount?.address) throw new Error('Missing account, kiosk or kiosk cap');

			if (!item?.objectId) throw new Error('Missing item.');

			const tx = new TransactionBlock();

			delist(tx, item.type, cap.kioskId, cap.objectId, item.objectId);

			return signAndExecute({ tx });
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
	const provider = useRpc();

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

			const policy = await queryTransferPolicy(provider, item.type);

			const policyId = policy[0]?.id;
			if (!policyId) {
				throw new Error(
					`This item doesn't have a Transfer Policy attached so it can't be traded through kiosk.`,
				);
			}

			const tx = new TransactionBlock();

			const environment = testnetEnvironment;

			const result = purchaseAndResolvePolicies(
				tx,
				item.type,
				item.listing.price,
				kioskId,
				item.objectId,
				policy[0],
				environment,
				{
					ownedKiosk: ownedKiosk.kioskId,
					ownedKioskCap: ownedKiosk.kioskCap,
				},
			);

			if (result.canTransfer)
				place(tx, item.type, ownedKiosk.kioskId, ownedKiosk.kioskCap, result.item);

			return await signAndExecute({ tx });
		},
		onSuccess,
		onError: onError || defaultOnError,
	});
}
