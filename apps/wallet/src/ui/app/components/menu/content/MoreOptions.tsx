// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { MenuLayout } from './MenuLayout';
import { Button } from '_app/shared/ButtonUI';
import { useNextMenuUrl } from '_components/menu/hooks';
import { ConfirmationModal } from '_src/ui/app/shared/ConfirmationModal';

export function MoreOptions() {
	const mainMenuUrl = useNextMenuUrl(true, '/');
	const [logoutInProgress, setLogoutInProgress] = useState(false);
	const [isLogoutDialogOpen, setIsLogoutDialogOpen] = useState(false);

	return (
		<MenuLayout title="More Options" back={mainMenuUrl}>
			<Button
				variant="warning"
				text="Logout"
				size="narrow"
				loading={logoutInProgress}
				disabled={isLogoutDialogOpen}
				onClick={async () => {
					setIsLogoutDialogOpen(true);
				}}
			/>
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
							// TODO: implement logout
							window.location.reload();
						} finally {
							setLogoutInProgress(false);
						}
					}
				}}
			/>
		</MenuLayout>
	);
}
