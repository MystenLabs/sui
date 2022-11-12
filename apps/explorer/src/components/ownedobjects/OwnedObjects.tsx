// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, getObjectFields, getObjectId } from '@mysten/sui.js';
import {
    useCallback,
    useEffect,
    useState,
    useContext,
    createContext,
} from 'react';

import { NetworkContext } from '../../context';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import {
    parseImageURL,
    parseObjectType,
    extractName,
} from '../../utils/objectUtils';
import { navigateWithUnknown } from '../../utils/searchUtil';
import {
    findDataFromID,
    findOwnedObjectsfromID,
} from '../../utils/static/searchUtil';
import { transformURL } from '../../utils/stringUtils';
import { type DataType } from './OwnedObjectConstants';
import OwnedObjectView from './views/OwnedObjectView';

import styles from './styles/OwnedObjects.module.css';

import { useRpc } from '~/hooks/useRpc';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';

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
    return IS_STATIC_ENV ? (
        <OwnedObjectStatic id={id} />
    ) : (
        <OwnedObjectAPI id={id} byAddress={byAddress} />
    );
}

const NavigateFunctionContext = createContext<(id: string) => () => void>(
    (id: string) => () => {}
);

function OwnedObjectStatic({ id }: { id: string }) {
    const navigate = useNavigateWithQuery();

    const navigateFn = useCallback(
        (id: string) => () => navigateWithUnknown(id, navigate),
        [navigate]
    );

    const objects = findOwnedObjectsfromID(id);

    if (objects) {
        const results = objects.map(({ objectId }) => {
            const entry = findDataFromID(objectId, undefined);
            return {
                id: entry?.id,
                Type: entry?.objType,
                Version: entry?.version,
                display: entry?.data?.contents?.display,
                balance: entry?.data?.contents?.balance
                    ? BigInt(entry?.data?.contents?.balance)
                    : undefined,
                _isCoin: entry?.data?.contents?.balance !== undefined,
                name: extractName(entry?.data?.contents),
            };
        });

        return (
            <NavigateFunctionContext.Provider value={navigateFn}>
                <OwnedObjectView results={results} />
            </NavigateFunctionContext.Provider>
        );
    } else {
        return <NoOwnedObjects />;
    }
}

function OwnedObjectAPI({ id, byAddress }: { id: string; byAddress: boolean }) {
    const [results, setResults] = useState(DATATYPE_DEFAULT);
    const [isLoaded, setIsLoaded] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const [network] = useContext(NetworkContext);
    const rpc = useRpc();
    const navigate = useNavigateWithQuery();
    const navigateFn = useCallback(
        (id: string) => () => navigateWithUnknown(id, navigate, network),
        [navigate, network]
    );

    useEffect(() => {
        setIsFail(false);
        setIsLoaded(false);
        const req = byAddress
            ? rpc.getObjectsOwnedByAddress(id)
            : rpc.getObjectsOwnedByObject(id);

        req.then((objects) => {
            const ids = objects.map(({ objectId }) => objectId);
            rpc.getObjectBatch(ids).then((results) => {
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

    if (isLoaded)
        return (
            <NavigateFunctionContext.Provider value={navigateFn}>
                <OwnedObjectView results={results} />
            </NavigateFunctionContext.Provider>
        );

    return <div className={styles.gray}>loading...</div>;
}

export default OwnedObject;
