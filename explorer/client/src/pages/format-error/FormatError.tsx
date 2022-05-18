// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';

const BadQuery = () => {
    const { id } = useParams();

    return (
        <ErrorResult
            id={id}
            errorMsg="search input was not an object ID, address, or transaction ID"
        />
    );
};

export default BadQuery;
