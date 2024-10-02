// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '_app/shared/ButtonUI';
import { useNextMenuUrl } from '_components/menu/hooks';
import { ampli } from '_src/shared/analytics/ampli';
import { persister } from '_src/ui/app/helpers/queryClient';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { ConfirmationModal } from '_src/ui/app/shared/ConfirmationModal';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';

import { MenuLayout } from './MenuLayout';

export function MoreOptions() {
	const mainMenuUrl = useNextMenuUrl(true, '/');
	const [isLogoutDialogOpen, setIsLogoutDialogOpen] = useState(false);
	const backgroundClient = useBackgroundClient();
	const queryClient = useQueryClient();
	const logoutMutation = useMutation({
		mutationKey: ['logout', 'clear wallet'],
		mutationFn: async () => {
			ampli.client.reset();
			queryClient.cancelQueries();
			queryClient.clear();
			await persister.removeClient();
			await backgroundClient.clearWallet();
		},
	});
	return (
		<MenuLayout title="More Options" back={mainMenuUrl}>
			<Button
				variant="warning"
				text="Logout"
				size="narrow"
				loading={logoutMutation.isPending}
				disabled={isLogoutDialogOpen}
				onClick={() => setIsLogoutDialogOpen(true)}
			/>
			<ConfirmationModal
				isOpen={isLogoutDialogOpen}
				confirmText="Logout"
				confirmStyle="outlineWarning"
				title="Are you sure you want to Logout?"
				hint="You will need to set up all your accounts again."
				onResponse={async (confirmed) => {
					setIsLogoutDialogOpen(false);
					if (confirmed) {
						await logoutMutation.mutateAsync(undefined, {
							onSuccess: () => {
								window.location.reload();
							},
						});
					}
				}}
			/>
		</MenuLayout>
	);
}
