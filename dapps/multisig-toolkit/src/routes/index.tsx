// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createBrowserRouter, Navigate } from 'react-router-dom';

import MultiSigCombinedSignatureGenerator from './combine-sigs';
import ExecuteTransaction from './execute-transaction';
import Help from './help';
import MultiSigAddressGenerator from './multisig-address';
import OfflineSigner from './offline-signer';
import { Root } from './root';
import SignatureAnalyzer from './signature-analyzer';

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
			{
				path: 'multisig-address',
				element: <MultiSigAddressGenerator />,
			},
			{
				path: 'combine-signatures',
				element: <MultiSigCombinedSignatureGenerator />,
			},
			{
				path: 'execute-transaction',
				element: <ExecuteTransaction />,
			},
			{
				path: 'help',
				element: <Help />,
			},
		],
	},
]);
