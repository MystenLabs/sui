// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionSender } from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { useEffect, useState } from 'react';
import { useLocation, useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { findDataFromID } from '../../utils/static/searchUtil';
import {
    instanceOfDataType,
    translate,
    type DataType,
} from './ObjectResultType';
import ObjectView from './views/ObjectView';

import { useRpc } from '~/hooks/useRpc';

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

function Fail({ objID }: { objID: string | undefined }) {
    return (
        <ErrorResult
            id={objID}
            errorMsg="Data could not be extracted on the following specified object ID"
        />
    );
}

function ObjectResultAPI({ objID }: { objID: string }) {
    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    const rpc = useRpc();

    useEffect(() => {
        rpc.getObject(objID)
            .then((objState) => {
                const resp: DataType = translate(objState) as DataType;

                const GENESIS_TX_DIGEST =
                    'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';

                if (
                    resp.data.tx_digest &&
                    resp.data.tx_digest === GENESIS_TX_DIGEST
                ) {
                    return {
                        ...resp,
                        publisherAddress: 'Genesis',
                    };
                }

                if (resp.objType === 'Move Package' && resp.data.tx_digest) {
                    return rpc
                        .getTransactionWithEffects(resp.data.tx_digest)
                        .then((txEff) => ({
                            ...resp,
                            publisherAddress: getTransactionSender(
                                txEff.certificate
                            ),
                        }))
                        .catch((err) => {
                            console.log(err);
                            return resp;
                        });
                }
                return resp;
            })
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

    return <div>Something went wrong</div>;
}

function ObjectResultStatic({ objID }: { objID: string }) {
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
}

function ObjectResult() {
    const { id: objID } = useParams();
    const { state } = useLocation();

    if (instanceOfDataType(state)) {
        return (
            <ErrorBoundary>
                <ObjectView data={state} />
            </ErrorBoundary>
        );
    }

    if (objID !== undefined) {
        return IS_STATIC_ENV ? (
            <ObjectResultStatic objID={objID} />
        ) : (
            <ErrorBoundary>
                <ObjectResultAPI objID={objID} />
            </ErrorBoundary>
        );
    }

    return <Fail objID={objID} />;
}

export { ObjectResult };
export type { DataType };
