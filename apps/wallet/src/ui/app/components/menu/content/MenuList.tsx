// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import {
	Account24,
	ArrowUpRight12,
	ArrowUpRight16,
	Domain24,
	Version24,
	CopyArchiveDoNotUse24,
} from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { MenuLayout } from './MenuLayout';
import MenuListItem from './MenuListItem';
import LoadingIndicator from '../../loading/LoadingIndicator';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import { Button } from '_app/shared/ButtonUI';
import { lockWallet } from '_app/wallet/actions';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAppDispatch, useAppSelector } from '_hooks';
import { ToS_LINK, FAQ_LINK } from '_src/shared/constants';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { useActiveAddress } from '_src/ui/app/hooks/useActiveAddress';
import { useAutoLockInterval } from '_src/ui/app/hooks/useAutoLockInterval';
import { useExplorerLink } from '_src/ui/app/hooks/useExplorerLink';
import { logout } from '_src/ui/app/redux/slices/account';
import { ConfirmationModal } from '_src/ui/app/shared/ConfirmationModal';
import { Link } from '_src/ui/app/shared/Link';
import FaucetRequestButton from '_src/ui/app/shared/faucet/FaucetRequestButton';
import { Text } from '_src/ui/app/shared/text';

function MenuList() {
	const accountUrl = useNextMenuUrl(true, '/accounts');
	const networkUrl = useNextMenuUrl(true, '/network');
	const autoLockUrl = useNextMenuUrl(true, '/auto-lock');
	const address = useActiveAddress();
	const { data: domainName } = useResolveSuiNSName(address);
	const apiEnv = useAppSelector((state) => state.app.apiEnv);
	const networkName = API_ENV_TO_INFO[apiEnv].name;
	const autoLockInterval = useAutoLockInterval();
	const version = Browser.runtime.getManifest().version;
	const dispatch = useAppDispatch();
	const navigate = useNavigate();
	const [logoutInProgress, setLogoutInProgress] = useState(false);
	const [isLogoutDialogOpen, setIsLogoutDialogOpen] = useState(false);
	const explorerAddress = useExplorerLink({
		type: ExplorerLinkType.address,
		address: address || '',
	});
	return (
		<>
			<MenuLayout title="Wallet Settings">
				<div className="flex flex-col divide-y divide-x-0 divide-solid divide-gray-45">
					<div className="flex flex-col mb-3">
						<MenuListItem
							to={accountUrl}
							icon={<Account24 />}
							title={'Accounts'}
							subtitle={domainName ?? (address ? formatAddress(address) : '')}
						/>
						{explorerAddress && (
							<Button
								variant="secondary"
								size="narrow"
								href={explorerAddress}
								text={
									<Text variant="bodySmall" weight="medium" color="steel-darker">
										View account on Sui Explorer
									</Text>
								}
								after={<ArrowUpRight16 className="text-steel w-4 h-4" />}
							/>
						)}
					</div>
					<MenuListItem
						to={networkUrl}
						icon={<Domain24 />}
						title="Network"
						subtitle={networkName}
					/>
					<MenuListItem
						to={autoLockUrl}
						icon={<Version24 />}
						title="Auto-lock"
						subtitle={autoLockInterval ? `${autoLockInterval} min` : <LoadingIndicator />}
					/>
					<MenuListItem
						icon={<CopyArchiveDoNotUse24 />}
						title={
							<div className="flex gap-1.5 items-center">
								FAQ
								<ArrowUpRight12 className="text-steel w-3 h-3" />
							</div>
						}
						href={FAQ_LINK}
					/>
				</div>
				<div className="flex flex-col items-stretch mt-2.5">
					<FaucetRequestButton variant="outline" />
				</div>
				<div className="flex-1" />
				<div className="flex flex-nowrap flex-row items-stretch gap-3 mt-2.5">
					<Button
						variant="outline"
						size="narrow"
						onClick={async () => {
							try {
								await dispatch(lockWallet()).unwrap();
								navigate('/locked', { replace: true });
							} catch (e) {
								// Do nothing
							}
						}}
						text="Lock Wallet"
					/>
					<Button
						variant="outline"
						text="Logout"
						size="narrow"
						loading={logoutInProgress}
						disabled={isLogoutDialogOpen}
						onClick={async () => {
							setIsLogoutDialogOpen(true);
						}}
					/>
				</div>
				<div className="px-2.5 flex flex-col items-center justify-center no-underline gap-3.75 mt-3.75">
					<Link
						href={ToS_LINK}
						text="Terms of service"
						after={<ArrowUpRight12 />}
						color="steelDark"
						weight="semibold"
					/>
					<Text variant="bodySmall" weight="medium" color="steel">
						Wallet Version v{version}
					</Text>
				</div>
			</MenuLayout>
			<ConfirmationModal
				isOpen={isLogoutDialogOpen}
				confirmText="Logout"
				confirmStyle="outlineWarning"
				title="Are you sure you want to Logout?"
				hint="You will need the 12-word Recovery Passphrase that was created when you first set up the wallet to log back in."
				onResponse={async (confirmed) => {
					setIsLogoutDialogOpen(false);
					if (confirmed) {
						setLogoutInProgress(true);
						try {
							await dispatch(logout());
							window.location.reload();
						} finally {
							setLogoutInProgress(false);
						}
					}
				}}
			/>
		</>
	);
}

export default MenuList;
