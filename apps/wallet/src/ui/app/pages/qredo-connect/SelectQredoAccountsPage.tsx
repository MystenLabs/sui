// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Overlay from '_components/overlay';
import { type Wallet } from '_src/shared/qredo-api';
import { ArrowRight16 } from '@mysten/icons';
import { useEffect, useState } from 'react';
import { Navigate, useLocation, useNavigate, useParams } from 'react-router-dom';

import { useAccountsFormContext } from '../../components/accounts/AccountsFormContext';
import { Button } from '../../shared/ButtonUI';
import { SelectQredoAccountsSummaryCard } from './components/SelectQredoAccountsSummaryCard';
import { useQredoUIPendingRequest } from './hooks';

export function SelectQredoAccountsPage() {
	const { id } = useParams();
	const { state } = useLocation();
	const navigate = useNavigate();
	const qredoRequestReviewed = !!state?.reviewed;
	const { data: qredoPendingRequest, isPending: isQredoRequestLoading } =
		useQredoUIPendingRequest(id);
	// do not call the api if user has not clicked continue in Qredo Connect Info page
	const fetchAccountsEnabled =
		!isQredoRequestLoading && (!qredoPendingRequest || qredoRequestReviewed);
	const [selectedAccounts, setSelectedAccounts] = useState<Wallet[]>([]);
	const shouldCloseWindow = (!isQredoRequestLoading && !qredoPendingRequest) || !id;
	const [, setAccountsFormValues] = useAccountsFormContext();
	useEffect(() => {
		if (shouldCloseWindow) {
			window.close();
		}
	}, [shouldCloseWindow]);
	if (qredoPendingRequest && !qredoRequestReviewed) {
		return <Navigate to="../" replace relative="path" />;
	}
	if (shouldCloseWindow) {
		return null;
	}
	return (
		<Overlay
			showModal
			title="Import Accounts"
			closeOverlay={() => {
				navigate(-1);
			}}
		>
			<div className="flex flex-col flex-1 flex-nowrap align-top overflow-x-hidden overflow-y-auto gap-3">
				<div className="flex flex-1 overflow-hidden">
					<SelectQredoAccountsSummaryCard
						fetchAccountsEnabled={fetchAccountsEnabled}
						qredoID={id}
						selectedAccounts={selectedAccounts}
						onChange={setSelectedAccounts}
					/>
				</div>
				<div>
					<Button
						size="tall"
						variant="primary"
						text="Continue"
						after={<ArrowRight16 />}
						disabled={!selectedAccounts?.length}
						onClick={async () => {
							setAccountsFormValues({ type: 'qredo', accounts: selectedAccounts, qredoID: id });
							navigate('/accounts/protect-account?accountType=qredo');
						}}
					/>
				</div>
			</div>
		</Overlay>
	);
}
