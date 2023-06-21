// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	ORIGINBYTE_KIOSK_MODULE,
	ORIGINBYTE_KIOSK_OWNER_TOKEN,
	useGetOwnedObjects,
	useRpcClient,
} from '@mysten/core';
import { type SuiAddress, TransactionBlock } from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';

import { useActiveAddress, useSigner } from '_src/ui/app/hooks';

export function useTransferKioskItem({
	objectId,
	objectType,
}: {
	objectId: string;
	objectType?: string;
}) {
	const rpc = useRpcClient();
	const signer = useSigner();
	const address = useActiveAddress();

	const { data: kioskOwnerTokens } = useGetOwnedObjects(address, {
		StructType: ORIGINBYTE_KIOSK_OWNER_TOKEN,
	});
	const kioskIds = kioskOwnerTokens?.pages
		.flatMap((page) => page.data)
		.map(
			(obj) => obj.data?.content && 'fields' in obj.data.content && obj.data.content.fields.kiosk,
		);

	return useMutation({
		mutationFn: async (to: SuiAddress) => {
			if (!to || !signer || !objectType) {
				throw new Error('Missing data');
			}

			const tx = new TransactionBlock();

			// fetch the kiosks for the active address
			const ownedKiosks = await rpc.multiGetObjects({
				ids: kioskIds!,
				options: { showContent: true },
			});

			// find the kiosk id containing the object that we want to transfer
			const kioskId = ownedKiosks.find(async (kiosk) => {
				if (!kiosk.data?.objectId) return false;
				const objects = await rpc.getDynamicFields({
					parentId: kiosk.data.objectId,
				});
				return objects.data.some((obj) => obj.objectId === objectId);
			})?.data?.objectId;

			if (!kioskId) throw new Error('failed to find kiosk containing object');

			// determine if the recipient address already owns a kiosk
			const recipientKiosks = await rpc.getOwnedObjects({
				owner: to,
				options: { showContent: true },
				filter: { StructType: ORIGINBYTE_KIOSK_OWNER_TOKEN },
			});

			const recipientKiosk = recipientKiosks.data[0]?.data;

			if (
				recipientKiosk &&
				recipientKiosk.content &&
				'fields' in recipientKiosk.content &&
				recipientKiosk.content.fields.kiosk
			) {
				const recipientKioskId = recipientKiosk.content.fields.kiosk;
				tx.moveCall({
					target: `${ORIGINBYTE_KIOSK_MODULE}::p2p_transfer`,
					typeArguments: [objectType],
					arguments: [tx.object(kioskId), tx.object(recipientKioskId), tx.pure(objectId)],
				});
			} else {
				tx.moveCall({
					target: `${ORIGINBYTE_KIOSK_MODULE}::p2p_transfer_and_create_target_kiosk`,
					typeArguments: [objectType],
					arguments: [tx.object(kioskId), tx.pure(to), tx.pure(objectId)],
				});
			}

			return signer.signAndExecuteTransactionBlock({
				transactionBlock: tx,
				options: {
					showInput: true,
					showEffects: true,
					showEvents: true,
				},
			});
		},
	});
}
