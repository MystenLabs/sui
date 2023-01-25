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
import {
    parseImageURL,
    parseObjectType,
    extractName,
} from '../../utils/objectUtils';
import { transformURL } from '../../utils/stringUtils';
import { type DataType } from './OwnedObjectConstants';
import OwnedObjectView from './views/OwnedObjectView';

import styles from './styles/OwnedObjects.module.css';

import { useRpc } from '~/hooks/useRpc';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';
import { navigateWithUnknown } from '~/utils/api/searchUtil';

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

const NavigateFunctionContext = createContext<(id: string) => () => void>(
    (id: string) => () => {}
);

function OwnedObject({ id, byAddress }: { id: string; byAddress: boolean }) {
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
            return rpc.getObjectBatch(ids).then((results) => {
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
