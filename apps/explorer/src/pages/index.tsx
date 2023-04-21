// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { wrapCreateBrowserRouter } from '@sentry/react';
import {
    createBrowserRouter,
    Navigate,
    useLocation,
    useParams,
} from 'react-router-dom';

import Address from './address';
import Checkpoint from './checkpoint';
import Epoch from './epoch';
import Home from './home/Home';
import Object from './object';
import Recent from './recent';
import TxBlock from './txblock';
import Validator from './validator';
import Validators from './validators';

import { Layout } from '~/components/Layout';

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
            { path: 'object/:id', element: <Object /> },
            { path: 'checkpoint/:id', element: <Checkpoint /> },
            { path: 'epoch/current', element: <Epoch /> },
            { path: 'epoch/:id', element: <Epoch /> },
            { path: 'txblock/:id', element: <TxBlock /> },
            { path: 'address/:id', element: <Address /> },
            { path: 'validators', element: <Validators /> },
            { path: 'validator/:id', element: <Validator /> },
        ],
    },
    // Support legacy routes:
    {
        path: '/transactions',
        element: <Navigate to="/recent" replace />,
    },
    {
        path: '/objects/:id',
        element: <RedirectWithId base="object" />,
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
        element: <RedirectWithId base="address" />,
    },
    // 404 route:
    { path: '*', element: <Navigate to="/" replace /> },
]);
