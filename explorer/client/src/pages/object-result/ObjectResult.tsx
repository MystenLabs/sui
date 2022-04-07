// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState } from 'react';
import { useLocation, useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/SuiRpcClient';
import ObjectLoaded from './ObjectLoaded';
import { type DataType } from './ObjectResultType';

const DATATYPE_DEFAULT: DataType = {
    id: '',
    category: '',
    owner: '',
    version: '',
    objType: '',
    data: {
        contents: {},
        owner: { ObjectOwner: [] },
        tx_digest: [],
    },
    loadState: 'pending',
};

function instanceOfDataType(object: any): object is DataType {
    return object && ['id', 'version', 'objType'].every((x) => x in object);
}

const Fail = ({ objID }: { objID: string | undefined }): JSX.Element => {
    return (
        <ErrorResult
            id={objID}
            errorMsg="There was an issue with the data on the following object"
        />
    );
};

const ObjectResultAPI = ({ objID }: { objID: string }): JSX.Element => {
    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    useEffect(() => {
        rpc.getObjectInfo(objID as string)
            .then((objState) => {
                setObjectState({
                    ...(objState as DataType),
                    loadState: 'loaded',
                });
            })
            .catch((error) => {
                console.log(error);
                setObjectState({ ...DATATYPE_DEFAULT, loadState: 'fail' });
            });
    }, [objID]);

    if (showObjectState.loadState === 'loaded') {
        return <ObjectLoaded data={showObjectState as DataType} />;
    }
    if (showObjectState.loadState === 'pending') {
        return (
            <div className={theme.pending}>Please wait for results to load</div>
        );
    }
    if (showObjectState.loadState === 'fail') {
        return <Fail objID={objID} />;
    }

    return <div>"Something went wrong"</div>;
};

const ObjectResultStatic = ({ objID }: { objID: string }): JSX.Element => {
    const { findDataFromID } = require('../../utils/static/searchUtil');
    const data = findDataFromID(objID, undefined);

    if (instanceOfDataType(data)) {
        return <ObjectLoaded data={data} />;
    } else {
        return <Fail objID={objID} />;
    }
};

const ObjectResult = (): JSX.Element => {
    const { id: objID } = useParams();
    const { state } = useLocation();

    if (instanceOfDataType(state)) {
        return <ObjectLoaded data={state} />;
    }

    if (objID !== undefined) {
        if (process.env.REACT_APP_DATA !== 'static') {
            return <ObjectResultAPI objID={objID} />;
        } else {
            return <ObjectResultStatic objID={objID} />;
        }
    }

    return <Fail objID={objID} />;
};

export { ObjectResult };
export type { DataType };
