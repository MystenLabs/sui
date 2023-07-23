// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Toaster } from 'react-hot-toast';
import { Outlet } from 'react-router-dom';
import PageLayout from '_pages/layout';

export function AccountsPage() {
	return (
		<PageLayout>
			<Outlet />
			<Toaster />
		</PageLayout>
	);
}
