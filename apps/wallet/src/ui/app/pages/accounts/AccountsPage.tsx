// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from 'react-router-dom';
import { AccountsFormProvider } from '../../components/accounts/AccountsFormContext';
import { Toaster } from '../../shared/toaster';
import PageLayout from '_pages/layout';

export function AccountsPage() {
	return (
		<AccountsFormProvider>
			<PageLayout>
				<Outlet />
				<Toaster bottomNavEnabled={false} />
			</PageLayout>
		</AccountsFormProvider>
	);
}
