// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createBrowserRouter, Navigate, Outlet } from 'react-router-dom';

export const router = createBrowserRouter([
	{
		path: '/',
		element: <Outlet />,
		children: [
			{
				path: '/',
				element: <Navigate to="offline-signer" replace />,
			},
		],
	},
]);
