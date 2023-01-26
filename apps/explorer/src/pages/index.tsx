// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { wrapCreateBrowserRouter } from '@sentry/react';
import {
    createBrowserRouter,
    Navigate,
    useLocation,
    useParams,
} from 'react-router-dom';

import AddressResult from './address-result/AddressResult';
import Home from './home/Home';
import { ObjectResult } from './object-result/ObjectResult';
import SearchResult from './search-result/SearchResult';
import SearchError from './searcherror/SearchError';
import TransactionResult from './transaction-result/TransactionResult';
import Transactions from './transactions/Transactions';
import { ValidatorDetails } from './validator/ValidatorDetails';
import { ValidatorPageResult } from './validators/Validators';

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
            { path: 'transactions', element: <Transactions /> },
            { path: 'object/:id', element: <ObjectResult /> },
            { path: 'transaction/:id', element: <TransactionResult /> },
            { path: 'address/:id', element: <AddressResult /> },
            { path: 'validators', element: <ValidatorPageResult /> },
            { path: 'validator/:id', element: <ValidatorDetails /> },
            { path: 'search-result/:id', element: <SearchResult /> },
            { path: 'error/:category/:id', element: <SearchError /> },
        ],
    },

    // Support legacy plural routes:
    {
        path: '/objects/:id',
        element: <RedirectWithId base="object" />,
    },
    {
        path: '/transactions/:id',
        element: <RedirectWithId base="transaction" />,
    },
    {
        path: '/addresses/:id',
        element: <RedirectWithId base="address" />,
    },
    // 404 route:
    { path: '*', element: <Navigate to="/" replace /> },
]);
