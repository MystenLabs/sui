// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { DAppPermissionsList } from '../../components/DAppPermissionsList';
import { SummaryCard } from '../../components/SummaryCard';
import { WalletListSelect } from '../../components/WalletListSelect';
import { useActiveAddress } from '../../hooks/useActiveAddress';
import { PageMainLayoutTitle } from '../../shared/page-main-layout/PageMainLayoutTitle';
import Loading from '_components/loading';
import { UserApproveContainer } from '_components/user-approve-container';
import { useAppDispatch, useAppSelector } from '_hooks';
import { permissionsSelectors, respondToPermissionRequest } from '_redux/slices/permissions';

import type { RootState } from '_redux/RootReducer';

import st from './SiteConnectPage.module.scss';

function SiteConnectPage() {
	const { requestID } = useParams();
	const permissionsInitialized = useAppSelector(({ permissions }) => permissions.initialized);
	const loading = !permissionsInitialized;
	const permissionSelector = useMemo(
		() => (state: RootState) =>
			requestID ? permissionsSelectors.selectById(state, requestID) : null,
		[requestID],
	);
	const dispatch = useAppDispatch();
	const permissionRequest = useAppSelector(permissionSelector);
	const activeAddress = useActiveAddress();
	const [accountsToConnect, setAccountsToConnect] = useState<SuiAddress[]>(() =>
		activeAddress ? [activeAddress] : [],
	);
	const handleOnSubmit = useCallback(
		async (allowed: boolean) => {
			if (requestID && accountsToConnect) {
				await dispatch(
					respondToPermissionRequest({
						id: requestID,
						accounts: allowed ? accountsToConnect : [],
						allowed,
					}),
				);
			}
		},
		[dispatch, requestID, accountsToConnect],
	);
	useEffect(() => {
		if (!loading && (!permissionRequest || permissionRequest.responseDate)) {
			window.close();
		}
	}, [loading, permissionRequest]);

	const parsedOrigin = useMemo(
		() => (permissionRequest ? new URL(permissionRequest.origin) : null),
		[permissionRequest],
	);

	const isSecure = parsedOrigin?.protocol === 'https:';
	const [displayWarning, setDisplayWarning] = useState(!isSecure);

	const handleHideWarning = useCallback(
		async (allowed: boolean) => {
			if (allowed) {
				setDisplayWarning(false);
			} else {
				await handleOnSubmit(false);
			}
		},
		[handleOnSubmit],
	);

	useEffect(() => {
		setDisplayWarning(!isSecure);
	}, [isSecure]);

	return (
		<Loading loading={loading}>
			{permissionRequest &&
				(displayWarning ? (
					<UserApproveContainer
						origin={permissionRequest.origin}
						originFavIcon={permissionRequest.favIcon}
						approveTitle="Continue"
						rejectTitle="Reject"
						onSubmit={handleHideWarning}
						isWarning
						addressHidden
					>
						<PageMainLayoutTitle title="Insecure Website" />
						<div className={st.warningWrapper}>
							<h1 className={st.warningTitle}>Your Connection is Not Secure</h1>
						</div>

						<div className={st.warningMessage}>
							This site requesting this wallet connection is not secure, and attackers might be
							trying to steal your information.
							<br />
							<br />
							Continue at your own risk.
						</div>
					</UserApproveContainer>
				) : (
					<UserApproveContainer
						origin={permissionRequest.origin}
						originFavIcon={permissionRequest.favIcon}
						approveTitle="Connect"
						rejectTitle="Reject"
						onSubmit={handleOnSubmit}
						addressHidden
						approveDisabled={!accountsToConnect.length}
					>
						<PageMainLayoutTitle title="Approve Connection" />
						<SummaryCard
							header="Permissions requested"
							body={<DAppPermissionsList permissions={permissionRequest.permissions} />}
						/>
						<WalletListSelect
							title="Connect Accounts"
							values={accountsToConnect}
							onChange={setAccountsToConnect}
						/>
					</UserApproveContainer>
				))}
		</Loading>
	);
}

export default SiteConnectPage;
