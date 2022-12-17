// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Navigate,
    Route,
    Routes,
    useLocation,
    useParams,
} from 'react-router-dom';

import AddressResult from '../address-result/AddressResult';
import Home from '../home/Home';
import { ObjectResult } from '../object-result/ObjectResult';
import SearchResult from '../search-result/SearchResult';
import SearchError from '../searcherror/SearchError';
import TransactionResult from '../transaction-result/TransactionResult';
import Transactions from '../transactions/Transactions';
import { ValidatorDetails } from '../validator/ValidatorDetails';
import { ValidatorPageResult } from '../validators/Validators';

function RedirectWithId({ base }: { base: string }) {
    const params = useParams();
    const { search } = useLocation();
    return <Navigate to={`/${base}/${params.id}${search}`} replace />;
}

function AppRoutes() {
    return (
        <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/transactions" element={<Transactions />} />
            <Route path="/object/:id" element={<ObjectResult />} />
            <Route path="/transaction/:id" element={<TransactionResult />} />
            <Route path="/address/:id" element={<AddressResult />} />
            <Route path="/validators" element={<ValidatorPageResult />} />
            <Route path="/search-result/:id" element={<SearchResult />} />
            <Route path="/error/:category/:id" element={<SearchError />} />
            <Route path="/validator/:id" element={<ValidatorDetails />} />

            {/* Support the existing plural routes: */}
            <Route
                path="/objects/:id"
                element={<RedirectWithId base="object" />}
            />
            <Route
                path="/transactions/:id"
                element={<RedirectWithId base="transaction" />}
            />
            <Route
                path="/addresses/:id"
                element={<RedirectWithId base="address" />}
            />

            <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
    );
}

export default AppRoutes;
