// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/sui.js/utils';
import { bytesToHex } from '@noble/hashes/utils';
import { useMutation } from '@tanstack/react-query';
import { useMemo } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import { HideShowDisplayBox } from '../../components/HideShowDisplayBox';
import { VerifyPasswordModal } from '../../components/accounts/VerifyPasswordModal';
import Alert from '../../components/alert';
import Loading from '../../components/loading';
import Overlay from '../../components/overlay';
import { useAccounts } from '../../hooks/useAccounts';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';

export function ExportAccountPage() {
	const { accountID } = useParams();
	const { data: allAccounts, isLoading } = useAccounts();
	const account = useMemo(
		() => allAccounts?.find(({ id }) => accountID === id) || null,
		[allAccounts, accountID],
	);
	const backgroundClient = useBackgroundClient();
	const exportMutation = useMutation({
		mutationKey: ['export-account', accountID],
		mutationFn: async (password: string) => {
			if (!account) {
				return null;
			}
			return await backgroundClient.exportAccountKeyPair({ password, accountID: account.id });
		},
	});
	const privateKey = useMemo(() => {
		if (exportMutation.data?.keyPair) {
			return `0x${bytesToHex(fromB64(exportMutation.data.keyPair.privateKey).slice(0, 32))}`;
		}
		return null;
	}, [exportMutation.data?.keyPair]);
	const navigate = useNavigate();
	if (!account && !isLoading) {
		return <Navigate to="../manage" replace />;
	}
	return (
		<Overlay title="Account Private Key" closeOverlay={() => navigate(-1)} showModal>
			<Loading loading={isLoading}>
				{privateKey ? (
					<div className="flex flex-col flex-nowrap items-stretch gap-3">
						<Alert>
							<div>Do not share your Private Key!</div>
							<div>It provides full control of your account.</div>
						</Alert>
						<HideShowDisplayBox value={privateKey} copiedMessage="Private key copied" />
					</div>
				) : (
					<VerifyPasswordModal
						open
						onVerify={async (password) => {
							await exportMutation.mutateAsync(password);
						}}
						onClose={() => navigate(-1)}
					/>
				)}
			</Loading>
		</Overlay>
	);
}
