// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Text } from '_app/shared/text';
import { AddressInput } from '_components/address-input';
import { ampli } from '_src/shared/analytics/ampli';
import { getSignerOperationErrorMessage } from '_src/ui/app/helpers/errorMessages';
import { useActiveAddress } from '_src/ui/app/hooks';
import { useActiveAccount } from '_src/ui/app/hooks/useActiveAccount';
import { useQredoTransaction } from '_src/ui/app/hooks/useQredoTransaction';
import { useSigner } from '_src/ui/app/hooks/useSigner';
import { QredoActionIgnoredByUser } from '_src/ui/app/QredoSigner';
import { useGetKioskContents, useSuiNSEnabled } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { ArrowRight16 } from '@mysten/icons';
import { Transaction } from '@mysten/sui/transactions';
import { isValidSuiNSName } from '@mysten/sui/utils';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Field, Form, Formik } from 'formik';
import { toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { useTransferKioskItem } from './useTransferKioskItem';
import { createValidationSchema } from './validation';

export function TransferNFTForm({
	objectId,
	objectType,
}: {
	objectId: string;
	objectType?: string | null;
}) {
	const activeAddress = useActiveAddress();
	const rpc = useSuiClient();
	const suiNSEnabled = useSuiNSEnabled();
	const validationSchema = createValidationSchema(rpc, suiNSEnabled, activeAddress || '', objectId);
	const activeAccount = useActiveAccount();
	const signer = useSigner(activeAccount);
	const queryClient = useQueryClient();
	const navigate = useNavigate();
	const { clientIdentifier, notificationModal } = useQredoTransaction();
	const { data: kiosk } = useGetKioskContents(activeAddress);
	const transferKioskItem = useTransferKioskItem({ objectId, objectType });
	const isContainedInKiosk = kiosk?.list.some((kioskItem) => kioskItem.data?.objectId === objectId);

	const transferNFT = useMutation({
		mutationFn: async (to: string) => {
			if (!to || !signer) {
				throw new Error('Missing data');
			}

			if (suiNSEnabled && isValidSuiNSName(to)) {
				const address = await rpc.resolveNameServiceAddress({
					name: to,
				});
				if (!address) {
					throw new Error('SuiNS name not found.');
				}
				to = address;
			}

			if (isContainedInKiosk) {
				return transferKioskItem.mutateAsync({ to, clientIdentifier });
			}

			const tx = new Transaction();
			tx.transferObjects([tx.object(objectId)], to);

			return signer.signAndExecuteTransactionBlock(
				{
					transactionBlock: tx,
					options: {
						showInput: true,
						showEffects: true,
						showEvents: true,
					},
				},
				clientIdentifier,
			);
		},
		onSuccess: (response) => {
			queryClient.invalidateQueries({ queryKey: ['object', objectId] });
			queryClient.invalidateQueries({ queryKey: ['get-kiosk-contents'] });
			queryClient.invalidateQueries({ queryKey: ['get-owned-objects'] });

			ampli.sentCollectible({ objectId });

			return navigate(
				`/receipt?${new URLSearchParams({
					txdigest: response.digest,
					from: 'nfts',
				}).toString()}`,
			);
		},
		onError: (error) => {
			if (error instanceof QredoActionIgnoredByUser) {
				navigate('/');
			} else {
				toast.error(
					<div className="max-w-xs overflow-hidden flex flex-col">
						<small className="text-ellipsis overflow-hidden">
							{getSignerOperationErrorMessage(error)}
						</small>
					</div>,
				);
			}
		},
	});

	return (
		<Formik
			initialValues={{
				to: '',
			}}
			validateOnMount
			validationSchema={validationSchema}
			onSubmit={({ to }) => transferNFT.mutateAsync(to)}
		>
			{({ isValid }) => (
				<Form autoComplete="off" className="h-full">
					<BottomMenuLayout className="h-full">
						<Content>
							<div className="flex gap-2.5 flex-col">
								<div className="px-2.5 tracking-wider">
									<Text variant="caption" color="steel" weight="semibold">
										Enter Recipient Address
									</Text>
								</div>
								<div className="w-full flex relative items-center flex-col">
									<Field
										component={AddressInput}
										allowNegative={false}
										name="to"
										placeholder="Enter Address"
									/>
								</div>
							</div>
						</Content>
						<Menu stuckClass="sendCoin-cta" className="w-full px-0 pb-0 mx-0 gap-2.5">
							<Button
								type="submit"
								variant="primary"
								loading={transferNFT.isPending}
								disabled={!isValid}
								size="tall"
								text="Send NFT Now"
								after={<ArrowRight16 />}
							/>
						</Menu>
					</BottomMenuLayout>
					{notificationModal}
				</Form>
			)}
		</Formik>
	);
}
