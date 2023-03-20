// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import {
    Coin,
    getObjectFields,
    getObjectId,
    PaginatedObjectsResponse,
    is,
} from '@mysten/sui.js';
import { useEffect, useState } from 'react';

import {
    parseImageURL,
    parseObjectType,
    extractName,
} from '../../utils/objectUtils';
import { transformURL } from '../../utils/stringUtils';
import { type DataType } from './OwnedObjectConstants';
import OwnedObjectView from './views/OwnedObjectView';

import styles from './styles/OwnedObjects.module.css';

const DATATYPE_DEFAULT: DataType = [
    {
        id: '',
        Type: '',
        _isCoin: false,
    },
];

function NoOwnedObjects() {
    return <div className={styles.fail}>Failed to find Owned Objects</div>;
}

function OwnedObject({ id, byAddress }: { id: string; byAddress: boolean }) {
    const [results, setResults] = useState(DATATYPE_DEFAULT);
    const [isLoaded, setIsLoaded] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const rpc = useRpcClient();

    useEffect(() => {
        setIsFail(false);
        setIsLoaded(false);
        const req = byAddress
            ? rpc.getOwnedObjects({ owner: id })
            : rpc.getDynamicFields({ parentId: id });

        req.then((objects) => {
            let ids: string[];
            if (is(objects, PaginatedObjectsResponse)) {
                ids = objects.data.map((resp) => getObjectId(resp));
            } else {
                ids = objects.data.map(({ objectId }) => objectId);
            }
            return rpc
                .multiGetObjects({
                    ids,
                    options: {
                        showType: true,
                        showContent: true,
                    },
                })
                .then((results) => {
                    setResults(
                        results
                            .filter(({ status }) => status === 'Exists')
                            .map(
                                (resp) => {
                                    const contents = getObjectFields(resp);
                                    const url = parseImageURL(contents);

                                    const name = extractName(contents);
                                    const objType = parseObjectType(resp);
                                    const balanceValue = Coin.getBalance(resp);
                                    return {
                                        id: getObjectId(resp),
                                        Type: objType,
                                        _isCoin: Coin.isCoin(resp),
                                        display: url
                                            ? transformURL(url)
                                            : undefined,
                                        balance: balanceValue,
                                        name: name,
                                    };
                                }
                                // TODO - add back version
                            )
                    );
                    setIsLoaded(true);
                });
        }).catch(() => setIsFail(true));
    }, [id, byAddress, rpc]);

    if (isFail) return <NoOwnedObjects />;

    if (isLoaded) return <OwnedObjectView results={results} />;

    return <div className={styles.gray}>loading...</div>;
}

export default OwnedObject;
