// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { getTransactionSender } from '@mysten/sui.js';
import { useEffect, useState } from 'react';
import { useLocation, useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import {
    instanceOfDataType,
    translate,
    type DataType,
} from './ObjectResultType';
import ObjectView from './views/ObjectView';

import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

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
        <Banner variant="error" spacing="lg" fullWidth>
            Data could not be extracted on the following specified object ID:{' '}
            {objID}
        </Banner>
    );
}

function ObjectResultAPI({ objID }: { objID: string }) {
    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    const rpc = useRpcClient();

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
                            publisherAddress: getTransactionSender(txEff),
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
        return <LoadingSpinner text="Loading data" />;
    }
    if (showObjectState.loadState === 'fail') {
        return <Fail objID={objID} />;
    }

    return <div>Something went wrong</div>;
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
        return (
            <ErrorBoundary>
                <ObjectResultAPI objID={objID} />
            </ErrorBoundary>
        );
    }

    return <Fail objID={objID} />;
}

export { ObjectResult };
export type { DataType };
