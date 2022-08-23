// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, getObjectFields, getObjectId } from '@mysten/sui.js';
import BN from 'bn.js';
import React, { useEffect, useState, useContext } from 'react';

import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import {
    parseImageURL,
    parseObjectType,
    extractName,
} from '../../utils/objectUtils';
import {
    findDataFromID,
    findOwnedObjectsfromID,
} from '../../utils/static/searchUtil';
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

const NoOwnedObjects = () => (
    <div className={styles.fail}>Failed to find Owned Objects</div>
);

const OwnedObject = ({ id, byAddress }: { id: string; byAddress: boolean }) =>
    IS_STATIC_ENV ? (
        <OwnedObjectStatic id={id} />
    ) : (
        <OwnedObjectAPI id={id} byAddress={byAddress} />
    );

function OwnedObjectStatic({ id }: { id: string }) {
    const objects = findOwnedObjectsfromID(id);

    if (objects) {
        const results = objects.map(({ objectId }) => {
            const entry = findDataFromID(objectId, undefined);
            const convertToBN = (balance: string): BN => new BN.BN(balance, 10);
            return {
                id: entry?.id,
                Type: entry?.objType,
                Version: entry?.version,
                display: entry?.data?.contents?.display,
                balance: convertToBN(entry?.data?.contents?.balance),
                _isCoin: entry?.data?.contents?.balance !== undefined,
                name: extractName(entry?.data?.contents),
            };
        });

        return <OwnedObjectView results={results} />;
    } else {
        return <NoOwnedObjects />;
    }
}

function OwnedObjectAPI({ id, byAddress }: { id: string; byAddress: boolean }) {
    const [results, setResults] = useState(DATATYPE_DEFAULT);
    const [isLoaded, setIsLoaded] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const [network] = useContext(NetworkContext);

    useEffect(() => {
        setIsFail(false);
        setIsLoaded(false);
        const req = byAddress
            ? rpc(network).getObjectsOwnedByAddress(id)
            : rpc(network).getObjectsOwnedByObject(id);

        req.then((objects) => {
            const ids = objects.map(({ objectId }) => objectId);
            rpc(network)
                .getObjectBatch(ids)
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
    }, [id, network, byAddress]);

    if (isFail) return <NoOwnedObjects />;

    if (isLoaded) return <OwnedObjectView results={results} />;

    return <div className={styles.gray}>loading...</div>;
}

export default OwnedObject;
