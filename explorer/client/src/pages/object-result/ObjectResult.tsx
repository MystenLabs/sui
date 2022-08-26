// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectId,
    getTransactions,
    getTransactionSender,
    getMoveCallTransaction,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import React, { useEffect, useState, useContext } from 'react';
import { useLocation, useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { findDataFromID } from '../../utils/static/searchUtil';
import {
    instanceOfDataType,
    translate,
    type DataType,
} from './ObjectResultType';
import ObjectView from './views/ObjectView';

const DATATYPE_DEFAULT: DataType = {
    id: '',
    category: '',
    owner: 'Immutable',
    version: '',
    objType: '',
    data: {
        contents: {},
        owner: { ObjectOwner: [] },
        tx_digest: '',
    },
    loadState: 'pending',
};

const Fail = ({ objID }: { objID: string | undefined }): JSX.Element => {
    return (
        <ErrorResult
            id={objID}
            errorMsg="Data could not be extracted on the following specified object ID"
        />
    );
};

// Get the data for the object ID and either:
// address that publishes a Package or
// module and package associated with token
function getObjectDataWithPackageAddress(objID: string, network: string) {
    return rpc(network)
        .getObject(objID as string)
        .then((objState) => {
            const resp: DataType = translate(objState) as DataType;
            if (resp.data.tx_digest) {
                return rpc(network)
                    .getTransactionWithEffects(resp.data.tx_digest)
                    .then((txEff) => {
                        if (resp.objType === 'Move Package') {
                            // If Package, then extract publisher address
                            return {
                                ...resp,
                                publisherAddress: getTransactionSender(
                                    txEff.certificate
                                ),
                            };
                        } else {
                            // If Token, then extract the module and package
                            const movecall = getMoveCallTransaction(
                                getTransactions(txEff.certificate)[0]
                            );
                            if (!movecall) return resp;
                            return {
                                ...resp,
                                module: movecall.module,
                                package: getObjectId(movecall.package),
                            };
                        }
                    })
                    .catch((err) => {
                        console.log(err);
                        // TODO: Not sure if I should show Genesis as Package Publisher or ignore it

                        if (resp.objType === 'Move Package') {
                            return {
                                ...(resp.owner === 'Immutable'
                                    ? { publisherAddress: 'Genesis' }
                                    : {}),
                                ...resp,
                            };
                        } else {
                            return resp;
                        }
                    });
            }
            return resp;
        });
}

const ObjectResultAPI = ({ objID }: { objID: string }): JSX.Element => {
    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getObjectDataWithPackageAddress(objID, network)
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
    }, [objID, network]);

    if (showObjectState.loadState === 'loaded') {
        return <ObjectView data={showObjectState as DataType} />;
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
    const data = findDataFromID(objID, undefined);

    if (instanceOfDataType(data)) {
        return <ObjectView data={data} />;
    } else {
        try {
            return <ObjectView data={translate(data)} />;
        } catch (err) {
            console.error("Couldn't parse data", err);
            Sentry.captureException(err);
            return <Fail objID={objID} />;
        }
    }
};

const ObjectResult = (): JSX.Element => {
    const { id: objID } = useParams();
    const { state } = useLocation();

    if (instanceOfDataType(state)) {
        return <ObjectView data={state} />;
    }

    if (objID !== undefined) {
        return IS_STATIC_ENV ? (
            <ObjectResultStatic objID={objID} />
        ) : (
            <ObjectResultAPI objID={objID} />
        );
    }

    return <Fail objID={objID} />;
};

export { ObjectResult };
export type { DataType };
