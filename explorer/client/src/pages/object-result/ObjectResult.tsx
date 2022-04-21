// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useLocation, useParams } from 'react-router-dom';
import { GetObjectInfoResponse } from 'sui.js';

import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/internetapi/DefaultRpcClient';
import { Loadable } from '../../utils/loadState';
import ObjectLoaded from './ObjectLoaded';

const DATATYPE_DEFAULT: Loadable<GetObjectInfoResponse> = {
    loadState: 'pending',
    status: 'NotExists',
    details: {
        object: undefined,
        objectRef: {
            digest: '',
            objectId: '',
            version: 0
        }
    }
};

function instanceOfDataType(object: any): object is GetObjectInfoResponse {
    return GetObjectInfoResponse.is(object);
}

const Fail = ({ objID }: { objID: string | undefined }): JSX.Element => {
    return (
        <ErrorResult
            id={objID}
            errorMsg="There was an issue with the data on the following object"
        />
    );
};

const ObjectResultInternetAPI = ({ objID }: { objID: string }): JSX.Element => {
    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    useEffect(() => {
        rpc.getObjectInfo(objID as string)
            .then((objState) => {
                setObjectState({
                    ...objState,
                    loadState: 'loaded'
                });
            })
            .catch((error) => {
                console.log(error);
                setObjectState({
                    ...DATATYPE_DEFAULT,
                    loadState: 'fail'
                });
            });
    }, [objID]);

    if (showObjectState.loadState === 'loaded') {
        return <ObjectLoaded data={showObjectState as Loadable<GetObjectInfoResponse>} />;
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
    // TODO - why are we using the location object as state ?
    const { state } = useLocation();

    if (instanceOfDataType(state)) {
        return <ObjectLoaded data={state} />;
    }

    if (objID !== undefined) {
        if (process.env.REACT_APP_DATA !== 'static') {
            return <ObjectResultInternetAPI objID={objID} />;
        } else {
            return <ObjectResultStatic objID={objID} />;
        }
    }

    return <Fail objID={objID} />;
};

export { ObjectResult };
