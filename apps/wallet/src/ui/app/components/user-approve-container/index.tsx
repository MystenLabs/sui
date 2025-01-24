// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ampli } from '_src/shared/analytics/ampli';
import { type PermissionType } from '_src/shared/messaging/messages/payloads/permissions';
import { Transaction } from '@mysten/sui/transactions';
import cn from 'clsx';
import { useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useAppSelector } from '../../hooks';
import { useAccountByAddress } from '../../hooks/useAccountByAddress';
import { type RootState } from '../../redux/RootReducer';
import { txRequestsSelectors } from '../../redux/slices/transaction-requests';
import { Button } from '../../shared/ButtonUI';
import { UnlockAccountButton } from '../accounts/UnlockAccountButton';
import { DAppInfoCard } from '../DAppInfoCard';
import { ScamOverlay } from '../known-scam-overlay';
import { RequestType } from '../known-scam-overlay/types';
import { useShowScamWarning } from '../known-scam-overlay/useShowScamWarning';

type UserApproveContainerProps = {
	children: ReactNode | ReactNode[];
	origin: string;
	originFavIcon?: string;
	rejectTitle: string;
	approveTitle: string;
	approveDisabled?: boolean;
	approveLoading?: boolean;
	onSubmit: (approved: boolean) => Promise<void>;
	isWarning?: boolean;
	addressHidden?: boolean;
	address?: string | null;
	scrollable?: boolean;
	blended?: boolean;
	permissions?: PermissionType[];
	checkAccountLock?: boolean;
};

export function UserApproveContainer({
	origin,
	originFavIcon,
	children,
	rejectTitle,
	approveTitle,
	approveDisabled = false,
	approveLoading = false,
	onSubmit,
	isWarning,
	addressHidden = false,
	address,
	permissions,
	checkAccountLock,
}: UserApproveContainerProps) {
	const [submitting, setSubmitting] = useState(false);
	const [scamOverlayDismissed, setScamOverlayDismissed] = useState(false);

	const handleDismissScamOverlay = () => {
		ampli.bypassedScamWarning({ hostname: new URL(origin).hostname });
		setScamOverlayDismissed(true);
	};

	const handleOnResponse = async (allowed: boolean) => {
		setSubmitting(true);
		await onSubmit(allowed);
		setSubmitting(false);
	};

	const { data: selectedAccount } = useAccountByAddress(address);
	const parsedOrigin = useMemo(() => new URL(origin), [origin]);
	const { requestID } = useParams();
	const requestSelector = useMemo(
		() => (state: RootState) =>
			requestID ? txRequestsSelectors.selectById(state, requestID) : null,
		[requestID],
	);
	const request = useAppSelector(requestSelector);

	const transaction = useMemo(() => {
		if (request && request.tx && 'data' in request.tx) {
			const transaction = Transaction.from(request.tx.data);
			transaction.setSender(request.tx.account);
			return transaction;
		}
	}, [request]);
	const message = request && request.tx && 'message' in request.tx ? request.tx.message : undefined;

	const {
		data: preflight,
		isPending: isDomainCheckLoading,
		isError,
	} = useShowScamWarning({
		url: parsedOrigin,
		requestType: message
			? RequestType.SIGN_MESSAGE
			: transaction
				? RequestType.SIGN_TRANSACTION
				: RequestType.CONNECT,
		transaction,
		requestId: requestID!,
	});

	return (
		<>
			{!scamOverlayDismissed && !!preflight && (
				<ScamOverlay
					preflight={preflight}
					onClickBack={() => handleOnResponse(false)}
					onClickContinue={handleDismissScamOverlay}
				/>
			)}
			<div className="flex flex-1 flex-col flex-nowrap h-full">
				<div className="flex-1 pb-0 flex flex-col">
					<DAppInfoCard
						name={parsedOrigin.host}
						url={origin}
						permissions={permissions}
						iconUrl={originFavIcon}
						connectedAddress={!addressHidden && address ? address : undefined}
						showSecurityWarning={isError}
					/>
					<div className="flex flex-1 flex-col px-6 bg-hero-darkest/5">{children}</div>
				</div>
				<div className="sticky bottom-0">
					<div
						className={cn(
							'bg-hero-darkest/5 backdrop-blur-lg py-4 px-5 flex items-center gap-2.5',
							{
								'flex-row-reverse': isWarning,
							},
						)}
					>
						{!checkAccountLock || !selectedAccount?.isLocked ? (
							<>
								<Button
									size="tall"
									variant="secondary"
									onClick={() => {
										handleOnResponse(false);
									}}
									disabled={submitting}
									text={rejectTitle}
								/>
								<Button
									size="tall"
									variant={isWarning ? 'secondary' : 'primary'}
									onClick={() => {
										handleOnResponse(true);
									}}
									disabled={approveDisabled}
									loading={submitting || approveLoading || isDomainCheckLoading}
									text={approveTitle}
								/>
							</>
						) : (
							<UnlockAccountButton account={selectedAccount} title="Unlock to Approve" />
						)}
					</div>
				</div>
			</div>
		</>
	);
}
