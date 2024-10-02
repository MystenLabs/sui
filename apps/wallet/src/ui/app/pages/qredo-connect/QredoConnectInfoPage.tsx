// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LabelValueItem } from '_components/LabelValueItem';
import { LabelValuesContainer } from '_components/LabelValuesContainer';
import { SummaryCard } from '_components/SummaryCard';
import { UserApproveContainer } from '_components/user-approve-container';
import { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Heading } from '../../shared/heading';
import { PageMainLayoutTitle } from '../../shared/page-main-layout/PageMainLayoutTitle';
import { Text } from '../../shared/text';
import { useQredoUIPendingRequest } from './hooks';
import { isUntrustedQredoConnect } from './utils';

export function QredoConnectInfoPage() {
	const { requestID } = useParams();
	const { data, isPending } = useQredoUIPendingRequest(requestID);
	const isUntrusted = !!data && isUntrustedQredoConnect(data);
	const [isUntrustedAccepted, setIsUntrustedAccepted] = useState(false);
	const navigate = useNavigate();
	const backgroundService = useBackgroundClient();
	useEffect(() => {
		if (!isPending && !data) {
			window.close();
		}
	}, [isPending, data]);
	if (isPending) {
		return null;
	}
	if (!data) {
		return null;
	}
	const showUntrustedWarning = isUntrusted && !isUntrustedAccepted;
	return (
		<>
			<PageMainLayoutTitle title="Qredo Accounts Setup" />
			<UserApproveContainer
				approveTitle="Continue"
				rejectTitle="Reject"
				isWarning={showUntrustedWarning}
				origin={data.origin}
				originFavIcon={data.originFavIcon}
				onSubmit={async (approved) => {
					if (approved) {
						if (showUntrustedWarning) {
							setIsUntrustedAccepted(true);
						} else {
							navigate('./select', { state: { reviewed: true } });
						}
					} else {
						await backgroundService.rejectQredoConnection({
							qredoID: data.id,
						});
						window.close();
					}
				}}
				addressHidden
			>
				<div className="pt-4">
					<SummaryCard
						header={showUntrustedWarning ? '' : 'More information'}
						body={
							showUntrustedWarning ? (
								<div className="flex flex-col gap-2.5">
									<Heading variant="heading6" weight="semibold" color="gray-90">
										Your Connection Is Not Secure
									</Heading>
									<Text variant="pBodySmall" weight="medium" color="steel-darker">
										If you connect your wallet with this site your data could be exposed to
										attackers.
									</Text>
									<div className="mt-2.5">
										<Text variant="pBodySmall" weight="medium" color="steel-darker">
											Click **Reject** if you don't trust this site. Continue at your own risk.
										</Text>
									</div>
								</div>
							) : (
								<LabelValuesContainer>
									<LabelValueItem label="Service Name" value={data.service} />
									<LabelValueItem label="Workspace" value={data.organization || '-'} />
									<LabelValueItem label="Token" value={data.partialToken} />
									<LabelValueItem label="API URL" value={data.apiUrl} />
								</LabelValuesContainer>
							)
						}
					/>
				</div>
			</UserApproveContainer>
		</>
	);
}
