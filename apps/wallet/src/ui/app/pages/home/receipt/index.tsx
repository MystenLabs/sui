// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Alert from '_components/alert';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { ReceiptCard } from '_src/ui/app/components/receipt-card';
import { useActiveAddress } from '_src/ui/app/hooks/useActiveAddress';
import { useUnlockedGuard } from '_src/ui/app/hooks/useUnlockedGuard';
import { useSuiClient } from '@mysten/dapp-kit';
import { Check32 } from '@mysten/icons';
import { type SuiTransactionBlockResponse } from '@mysten/sui/client';
import { useQuery } from '@tanstack/react-query';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useLocation, useNavigate, useSearchParams } from 'react-router-dom';

function ReceiptPage() {
	const location = useLocation();
	const [searchParams] = useSearchParams();
	const [showModal, setShowModal] = useState(true);
	const activeAddress = useActiveAddress();

	// get tx results from url params
	const transactionId = searchParams.get('txdigest');
	const fromParam = searchParams.get('from');
	const client = useSuiClient();

	const { data, isPending, isError } = useQuery<SuiTransactionBlockResponse>({
		queryKey: ['transactions-by-id', transactionId],
		queryFn: async () => {
			return client.getTransactionBlock({
				digest: transactionId!,
				options: {
					showBalanceChanges: true,
					showObjectChanges: true,
					showInput: true,
					showEffects: true,
					showEvents: true,
				},
			});
		},
		enabled: !!transactionId,
		retry: 8,
		initialData: location.state?.response,
	});

	const navigate = useNavigate();
	// return to previous route or from param if available
	const closeReceipt = useCallback(() => {
		fromParam ? navigate(`/${fromParam}`) : navigate(-1);
	}, [fromParam, navigate]);

	const pageTitle = useMemo(() => {
		if (data) {
			const executionStatus = data.effects?.status.status;

			// TODO: Infer out better name:
			const transferName = 'Transaction';

			return `${executionStatus === 'success' ? transferName : 'Transaction Failed'}`;
		}

		return 'Transaction Failed';
	}, [/*activeAddress,*/ data]);

	const isGuardLoading = useUnlockedGuard();

	if (!transactionId || !activeAddress) {
		return <Navigate to="/transactions" replace={true} />;
	}

	return (
		<Loading loading={isPending || isGuardLoading}>
			<Overlay
				showModal={showModal}
				setShowModal={setShowModal}
				title={pageTitle}
				closeOverlay={closeReceipt}
				closeIcon={<Check32 fill="currentColor" className="text-sui-light w-8 h-8" />}
			>
				{isError ? (
					<div className="mb-2 h-fit">
						<Alert>Something went wrong</Alert>
					</div>
				) : null}

				{data && <ReceiptCard txn={data} activeAddress={activeAddress} />}
			</Overlay>
		</Loading>
	);
}

export default ReceiptPage;
