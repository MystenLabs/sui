// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { wrapCreateBrowserRouter } from '@sentry/react';
import { createBrowserRouter, Navigate, useLocation, useParams } from 'react-router-dom';

import CheckpointDetail from './checkpoints/CheckpointDetail';
import EpochDetail from './epochs/EpochDetail';
import Home from './home/Home';
import { Recent } from './recent';
import TransactionResult from './transaction-result/TransactionResult';
import { ValidatorDetails } from './validator/ValidatorDetails';
import { ValidatorPageResult } from './validators/Validators';
import { Layout } from '~/components/Layout';
import { IdPage } from '~/pages/id-page';

function RedirectWithId({ base }: { base: string }) {
	const params = useParams();
	const { search } = useLocation();
	return <Navigate to={`/${base}/${params.id}${search}`} replace />;
}

const sentryCreateBrowserRouter = wrapCreateBrowserRouter(createBrowserRouter);

export const router = sentryCreateBrowserRouter([
	{
		path: '/',
		element: <Layout />,
		children: [
			{ path: '/', element: <Home /> },
			{ path: 'recent', element: <Recent /> },
			{ path: 'checkpoint/:id', element: <CheckpointDetail /> },
			{ path: 'epoch/current', element: <EpochDetail /> },
			{ path: 'txblock/:id', element: <TransactionResult /> },
			{ path: 'epoch/:id', element: <EpochDetail /> },
			{ path: 'validators', element: <ValidatorPageResult /> },
			{ path: 'validator/:id', element: <ValidatorDetails /> },
			{ path: 'id/:id', element: <IdPage /> },
		],
	},
	{
		path: '/transactions',
		element: <Navigate to="/recent" replace />,
	},
	// Support legacy routes:
	{
		path: '/objects/:id',
		element: <RedirectWithId base="id" />,
	},
	{
		path: '/transaction/:id',
		element: <RedirectWithId base="txblock" />,
	},
	{
		path: '/transactions/:id',
		element: <RedirectWithId base="txblock" />,
	},
	{
		path: '/addresses/:id',
		element: <RedirectWithId base="id" />,
	},
	{
		path: '/address/:id',
		element: <RedirectWithId base="id" />,
	},
	{
		path: '/object/:id',
		element: <RedirectWithId base="id" />,
	},
	// 404 route:
	{ path: '*', element: <Navigate to="/" replace /> },
]);
