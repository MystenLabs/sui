// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { useMutation } from '@tanstack/react-query';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import { VerifyPasswordModal } from '../../components/accounts/VerifyPasswordModal';
import Alert from '../../components/alert';
import { HideShowDisplayBox } from '../../components/HideShowDisplayBox';
import Loading from '../../components/loading';
import Overlay from '../../components/overlay';
import { useAccounts } from '../../hooks/useAccounts';

export function ExportAccountPage() {
	const { accountID } = useParams();
	const { data: allAccounts, isPending } = useAccounts();
	const account = allAccounts?.find(({ id }) => accountID === id) || null;
	const backgroundClient = useBackgroundClient();
	const exportMutation = useMutation({
		mutationKey: ['export-account', accountID],
		mutationFn: async (password: string) => {
			if (!account) {
				return null;
			}
			return (
				await backgroundClient.exportAccountKeyPair({
					password,
					accountID: account.id,
				})
			).keyPair;
		},
	});
	const navigate = useNavigate();
	if (!account && !isPending) {
		return <Navigate to="/accounts/manage" replace />;
	}
	return (
		<Overlay title="Account Private Key" closeOverlay={() => navigate(-1)} showModal>
			<Loading loading={isPending}>
				{exportMutation.data ? (
					<div className="flex flex-col flex-nowrap items-stretch gap-3">
						<Alert>
							<div>Do not share your Private Key!</div>
							<div>It provides full control of your account.</div>
						</Alert>
						<HideShowDisplayBox value={exportMutation.data} copiedMessage="Private key copied" />
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
