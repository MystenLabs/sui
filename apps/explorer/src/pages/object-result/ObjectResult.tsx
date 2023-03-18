// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { getTransactionSender } from '@mysten/sui.js';
import { useEffect, useState } from 'react';
import { useLocation, useParams } from 'react-router-dom';
import { useGetObject } from '../../hooks/useGetObject';
import { extractName } from '../../utils/objectUtils';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { PageHeader } from '~/ui/PageHeader';
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

const GENESIS_TX_DIGEST = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';
const PACKAGE_TYPE_NAME = 'Move Package';

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
        rpc.getObject({
            id: objID,
            options: {
                showType: true,
                showContent: true,
                showOwner: true,
                showPreviousTransaction: true,
                showStorageRebate: true,
            },
        })
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
                        .getTransaction({
                            digest: resp.data.tx_digest,
                            options: {
                                showInput: true,
                            },
                        })
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


export function ObjectResult() {
    const { id: objID } = useParams();
    const { state } = useLocation();
    const { data, isLoading, isError } = useGetObject(objID!);
    if (isLoading) {
        return <LoadingSpinner text="Loading data" />;
    }

    if (isError) {
        return <Fail objID={objID} />;
    }

   if (instanceOfDataType(state)) {
    return (
        <ErrorBoundary>
            <ObjectView data={state} />
        </ErrorBoundary>
    );
 
}
    const resp = translate(data);
    const name = extractName(resp.data?.contents);
    const isPackage = resp.objType === PACKAGE_TYPE_NAME;
    // Handle Package, Object, and Module, Genesis

    // Return move Package view
    // Return move Object view

    return (<div className="mt-5 mb-10">
        <PageHeader
                    type={isPackage ? 'Package' : 'Object'}
                    title={resp.id}
                    subtitle={name}
                />

        <ErrorBoundary>
            <div className='mt-10'>
                <ObjectView data={resp} />
                </div>
        </ErrorBoundary>
    </div>)




  
}

export type { DataType };
