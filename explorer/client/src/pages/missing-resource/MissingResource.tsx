// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';

const MissingResource = () => {
    const { id } = useParams();

    return (
        <ErrorResult
            id={id}
            errorMsg="Data on the following query could not be found"
        />
    );
};

export default MissingResource;
