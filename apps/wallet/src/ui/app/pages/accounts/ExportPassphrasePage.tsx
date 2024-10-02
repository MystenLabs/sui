// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Alert from '_components/alert';
import { HideShowDisplayBox } from '_components/HideShowDisplayBox';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import { VerifyPasswordModal } from '../../components/accounts/VerifyPasswordModal';
import Loading from '../../components/loading';
import Overlay from '../../components/overlay';
import { useAccountSources } from '../../hooks/useAccountSources';
import { useExportPassphraseMutation } from '../../hooks/useExportPassphraseMutation';

export function ExportPassphrasePage() {
	const { accountSourceID } = useParams();
	const { data: allAccountSources, isPending } = useAccountSources();
	const accountSource = allAccountSources?.find(({ id }) => id === accountSourceID) || null;
	const navigate = useNavigate();
	const exportMutation = useExportPassphraseMutation();
	if (!isPending && accountSource?.type !== 'mnemonic') {
		return <Navigate to="/accounts/manage" />;
	}
	return (
		<Overlay title="Export Passphrase" closeOverlay={() => navigate(-1)} showModal>
			<Loading loading={isPending}>
				{exportMutation.data ? (
					<div className="flex flex-col gap-3 min-w-0">
						<Alert>
							<div className="break-normal">Do not share your Passphrase!</div>
							<div className="break-normal">
								It provides full control of all accounts derived from it.
							</div>
						</Alert>
						<HideShowDisplayBox value={exportMutation.data} copiedMessage="Passphrase copied" />
					</div>
				) : (
					<VerifyPasswordModal
						open
						onVerify={async (password) => {
							await exportMutation.mutateAsync({ password, accountSourceID: accountSource!.id });
						}}
						onClose={() => navigate(-1)}
					/>
				)}
			</Loading>
		</Overlay>
	);
}
