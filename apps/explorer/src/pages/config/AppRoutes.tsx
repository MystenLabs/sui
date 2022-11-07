// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Navigate, Route, Routes } from 'react-router-dom';

import AddressResult from '../address-result/AddressResult';
import Home from '../home/Home';
import { ObjectResult } from '../object-result/ObjectResult';
import SearchResult from '../search-result/SearchResult';
import SearchError from '../searcherror/SearchError';
import TransactionResult from '../transaction-result/TransactionResult';
import Transactions from '../transactions/Transactions';
import { ValidatorPageResult } from '../validators/Validators';

function AppRoutes() {
    return (
        <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/transactions" element={<Transactions />} />
            <Route path="/objects/:id" element={<ObjectResult />} />
            <Route path="/transactions/:id" element={<TransactionResult />} />
            <Route path="/addresses/:id" element={<AddressResult />} />
            <Route path="/validators" element={<ValidatorPageResult />} />
            <Route path="/search-result/:id" element={<SearchResult />} />
            <Route path="/error/:category/:id" element={<SearchError />} />
            <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
    );
}

export default AppRoutes;
