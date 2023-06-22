// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createBrowserRouter, Navigate } from 'react-router-dom';
import OfflineSigner from './offline-signer';
import SignatureAnalyzer from './signature-analyzer';
import { Root } from './root';

export const router = createBrowserRouter([
	{
		path: '/',
		element: <Root />,
		children: [
			{
				path: '/',
				element: <Navigate to="offline-signer" replace />,
			},
			{
				path: 'offline-signer',
				element: <OfflineSigner />,
			},
			{
				path: 'signature-analyzer',
				element: <SignatureAnalyzer />,
			},
		],
	},
]);
