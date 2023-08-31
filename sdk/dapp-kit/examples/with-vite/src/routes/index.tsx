// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createBrowserRouter, Outlet } from 'react-router-dom';
import { WalletPage } from './WalletPage.js';

export const router = createBrowserRouter([
	{
		path: '/',
		element: <Outlet />,
		children: [
			{
				path: '/',
				element: <WalletPage />,
			},
		],
	},
]);
