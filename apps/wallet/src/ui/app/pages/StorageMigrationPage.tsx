// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { toast } from 'react-hot-toast';

import LoadingIndicator from '../components/loading/LoadingIndicator';
import { PasswordInputDialog } from '../components/PasswordInputDialog';
import { useBackgroundClient } from '../hooks/useBackgroundClient';
import { useStorageMigrationStatus } from '../hooks/useStorageMigrationStatus';
import { CardLayout } from '../shared/card-layout';
import { Toaster } from '../shared/toaster';

export function StorageMigrationPage() {
	const { data } = useStorageMigrationStatus();
	const backgroundClient = useBackgroundClient();
	const migrationMutation = useMutation({
		mutationKey: ['do storage migration'],
		mutationFn: ({ password }: { password: string }) =>
			backgroundClient.doStorageMigration({ password }),
		onSuccess: () => {
			toast.success('Storage migration done');
		},
	});
	if (!data || data === 'ready') {
		return null;
	}
	return (
		<>
			<CardLayout
				title={data === 'inProgress' ? 'Storage migration in progress, please wait' : ''}
				subtitle={data === 'required' ? 'Storage migration is required' : ''}
				icon="sui"
			>
				{data === 'required' && !migrationMutation.isSuccess ? (
					<PasswordInputDialog
						onPasswordVerified={async (password) => {
							await migrationMutation.mutateAsync({ password });
						}}
						title="Please insert your wallet password"
						legacyAccounts
					/>
				) : (
					<div className="flex flex-1 items-center">
						<LoadingIndicator />
					</div>
				)}
			</CardLayout>
			<Toaster />
		</>
	);
}
