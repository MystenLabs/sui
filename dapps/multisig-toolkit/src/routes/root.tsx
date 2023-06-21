// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Header } from '@/components/header';
import { Warning } from '@/components/warning';
import { Outlet } from 'react-router-dom';

export function Root() {
	return (
		<div>
			<Header />
			<div className="container py-8">
				<Outlet />
			</div>
			<Warning />
		</div>
	);
}
