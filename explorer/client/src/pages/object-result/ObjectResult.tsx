//import 'ace-builds/src-noconflict/theme-github';
import React, { useEffect, useState } from 'react';
//import AceEditor from 'react-ace';
import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import {
    type AddressOwner,
    DefaultRpcClient,
} from '../../utils/internetapi/rpc';
import ObjectLoaded from './ObjectLoaded';

type DataType = {
    id: string;
    category?: string;
    owner: string | AddressOwner;
    version: string;
    readonly?: string;
    objType: string;
    name?: string;
    ethAddress?: string;
    ethTokenId?: string;
    contract_id?: { bytes: string };
    data: {
        contents: {
            [key: string]: any;
        };
        owner?: { ObjectOwner: [] };
        tx_digest?: number[] | string;
    };
    loadState?: string;
};

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

const ObjectResult = (): JSX.Element => {
    const { id: objID } = useParams();

    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    const rpc = DefaultRpcClient;

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
    }, [objID, rpc]);

    if (showObjectState.loadState === 'loaded') {
        return <ObjectLoaded data={showObjectState as DataType} />;
    }
    if (showObjectState.loadState === 'pending') {
        return (
            <div className={theme.pending}>Please wait for results to load</div>
        );
    }
    if (showObjectState.loadState === 'fail') {
        return (
            <ErrorResult
                id={objID}
                errorMsg="There was an issue with the data on the following object"
            />
        );
    }

    return <div>"Something went wrong"</div>;
};

export { ObjectResult };
export type { DataType };
