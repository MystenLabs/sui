// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { toast } from 'react-hot-toast';

import { type DAppEntry } from './SuiApp';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Text } from '../../shared/text';
import { DAppInfoCard } from '../DAppInfoCard';
import { DAppPermissionsList } from '../DAppPermissionsList';
import { SummaryCard } from '../SummaryCard';
import { WalletListSelect } from '../WalletListSelect';
import Overlay from '_components/overlay';
import { useAppSelector } from '_hooks';
import { permissionsSelectors } from '_redux/slices/permissions';
import { ampli } from '_src/shared/analytics/ampli';

export interface DisconnectAppProps extends Omit<DAppEntry, 'description' | 'tags'> {
	permissionID: string;
	setShowDisconnectApp: (showModal: boolean) => void;
}

function DisconnectApp({
	name,
	icon,
	link,
	permissionID,
	setShowDisconnectApp,
}: DisconnectAppProps) {
	const [accountsToDisconnect, setAccountsToDisconnect] = useState<string[]>([]);
	const permission = useAppSelector((state) =>
		permissionsSelectors.selectById(state, permissionID),
	);
	useEffect(() => {
		if (permission && !permission.allowed) {
			setShowDisconnectApp(false);
		}
	}, [permission, setShowDisconnectApp]);
	const connectedAccounts = useMemo(
		() => (permission?.allowed && permission.accounts) || [],
		[permission],
	);
	const backgroundClient = useBackgroundClient();
	const disconnectMutation = useMutation({
		mutationFn: async () => {
			const origin = permission?.origin;
			if (!origin) {
				throw new Error('Failed, origin not found');
			}

			await backgroundClient.disconnectApp(origin, accountsToDisconnect);
			await backgroundClient.sendGetPermissionRequests();
			ampli.disconnectedApplication({
				sourceFlow: 'Application page',
				disconnectedAccounts: accountsToDisconnect.length || 1,
				applicationName: permission.name,
				applicationUrl: origin,
			});
		},
		onSuccess: () => {
			toast.success('Disconnected successfully');
			setShowDisconnectApp(false);
		},
		onError: () => toast.error('Disconnect failed'),
	});
	if (!permission) {
		return null;
	}
	return (
		<Overlay showModal setShowModal={setShowDisconnectApp} title="Connection Active">
			<div className="flex flex-col flex-nowrap items-stretch flex-1 gap-3.75">
				<DAppInfoCard name={name} iconUrl={icon} url={link} />
				<SummaryCard
					header="Permissions given"
					body={<DAppPermissionsList permissions={permission.permissions} />}
				/>
				{connectedAccounts.length > 1 ? (
					<WalletListSelect
						title="Connected Accounts"
						visibleValues={connectedAccounts}
						values={accountsToDisconnect}
						onChange={setAccountsToDisconnect}
						mode="disconnect"
						disabled={disconnectMutation.isLoading}
					/>
				) : (
					<SummaryCard
						header="Connected Account"
						body={
							<Text variant="body" color="steel-dark" weight="semibold" mono>
								{connectedAccounts[0] ? formatAddress(connectedAccounts[0]) : null}
							</Text>
						}
					/>
				)}
				<div className="sticky flex items-end flex-1 -bottom-5 bg-white pt-1 pb-5">
					<Button
						size="tall"
						variant="warning"
						text={
							connectedAccounts.length === 1
								? 'Disconnect'
								: accountsToDisconnect.length === 0 ||
								  connectedAccounts.length === accountsToDisconnect.length
								? 'Disconnect All'
								: 'Disconnect Selected'
						}
						loading={disconnectMutation.isLoading}
						onClick={() => disconnectMutation.mutate()}
					/>
				</div>
			</div>
		</Overlay>
	);
}

export default DisconnectApp;
