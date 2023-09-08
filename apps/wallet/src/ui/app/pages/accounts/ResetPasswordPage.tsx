// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from 'react-router-dom';

import PageLayout from '_pages/layout';

export function ResetPasswordPage() {
	return (
		<PageLayout>
			<Outlet />
		</PageLayout>
	);
}
