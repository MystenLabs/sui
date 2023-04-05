// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import {
    type SuiObjectResponse,
    getObjectType,
    getObjectOwner,
    Coin,
} from '@mysten/sui.js';
import { useEffect, useState } from 'react';

import PaginationContainer from '../PaginationContainer/PaginationContainer';
import OwnedObject from './components/OwnedObject';

export const OBJECTS_PER_PAGE: number = 6;

function OwnerCoins({ id }: { id: string }) {
    const [results, setResults] = useState<SuiObjectResponse[]>([]);
    const [isLoaded, setIsLoaded] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const [currentPage, setCurrentPage] = useState(1);
    const rpc = useRpcClient();

    useEffect(() => {
        rpc.getOwnedObjects({ owner: id })
            .then((objects) => {
                const ids: string[] = objects.data.map(
                    (obj) => obj.data?.objectId ?? ''
                );
                rpc.multiGetObjects({
                    ids,
                    options: {
                        showType: true,
                        showContent: true,
                        showDisplay: true,
                    },
                }).then((results) => {
                    setResults(
                        results.filter((resp) => {
                            if (getObjectType(resp) === 'moveObject') {
                                const owner = getObjectOwner(resp);
                                const addressOwner =
                                    owner &&
                                    owner !== 'Immutable' &&
                                    'AddressOwner' in owner
                                        ? owner.AddressOwner
                                        : null;
                                return (
                                    resp !== undefined && addressOwner === id
                                );
                            }
                            return resp !== undefined && !Coin.isCoin(resp);
                        })
                    );
                    setIsLoaded(true);
                });
            })
            .catch((err) => {
                setIsFail(true);
            });
    }, [id, rpc]);

    return (
        <PaginationContainer
            heading="NFTs"
            isLoaded={isLoaded}
            isFail={isFail}
            itemsPerPage={OBJECTS_PER_PAGE}
            paginatedContent={
                <div className="flex flex-wrap">
                        {results
                            .slice(
                                (currentPage - 1) * OBJECTS_PER_PAGE,
                                currentPage * OBJECTS_PER_PAGE
                            )
                            .map((obj) => <OwnedObject obj={obj} />)}
                    </div>
            }
            totalItems={results.length}
            currentPage={currentPage}
            setCurrentPage={setCurrentPage}
        />
    );
}

export default OwnerCoins;
