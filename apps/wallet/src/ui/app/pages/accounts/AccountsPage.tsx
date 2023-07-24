// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from 'react-router-dom';
import { Toaster } from '../../shared/toaster';
import PageLayout from '_pages/layout';

export function AccountsPage() {
	return (
		<PageLayout>
			<Outlet />
			<Toaster bottomNavEnabled={false} />
		</PageLayout>
	);
}
