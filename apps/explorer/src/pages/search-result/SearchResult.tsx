// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidTransactionDigest, isValidSuiAddress } from '@mysten/sui.js';
import { useEffect, useState, useContext } from 'react';
import { useParams } from 'react-router-dom';

import Longtext from '../../components/longtext/Longtext';
import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { isGenesisLibAddress } from '../../utils/api/searchUtil';

import styles from './SearchResult.module.css';

import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

type SearchDataType = {
    resultdata: any[];
    loadState?: 'loaded' | 'pending' | 'fail';
};

const initState: SearchDataType = {
    loadState: 'pending',
    resultdata: [],
};

const querySearchParams = async (input: string, network: Network | string) => {
    const version = await rpc(network).getRpcApiVersion();
    let searchPromises = [];
    if (
        isValidTransactionDigest(
            input,
            version?.major === 0 && version?.minor < 18 ? 'base64' : 'base58'
        )
    ) {
        searchPromises.push(
            rpc(network)
                .getTransactionWithEffects(input)
                .then((data) => [
                    {
                        category: 'transaction',
                        data: data,
                    },
                ])
        );
    } else if (isValidSuiAddress(input) || isGenesisLibAddress(input)) {
        const addrObjPromise = Promise.allSettled([
            rpc(network)
                .getObjectsOwnedByAddress(input)
                .then((data) => {
                    if (data.length <= 0)
                        throw new Error('No objects for Address');

                    return {
                        category: 'address',
                        data: data,
                    };
                }),
            rpc(network)
                .getObject(input)
                .then((data) => {
                    if (data.status !== 'Exists') {
                        throw new Error('no object found');
                    }
                    return {
                        category: 'object',
                        data: data,
                    };
                }),
        ]).then((results) => {
            // return only the successful results
            const searchResult = results
                .filter((result: any) => result.status === 'fulfilled')
                .map((data: any) => data.value);
            return searchResult;
        });
        searchPromises.push(addrObjPromise);
    }
    return Promise.any(searchPromises);
};

function SearchResult() {
    const { id } = useParams();
    const [network] = useContext(NetworkContext);
    const [resultData, setResultData] = useState(initState);
    useEffect(() => {
        if (id == null) {
            return;
        }
        querySearchParams(id, network)
            .then((data: any) => {
                setResultData({
                    resultdata: [...data],
                    loadState: 'loaded',
                });
            })
            .catch((error) => {
                setResultData({
                    loadState: 'fail',
                    resultdata: [],
                });
            });
    }, [id, network]);

    if (resultData.loadState === 'pending') {
        return <LoadingSpinner text="Loading..." />;
    }

    if (
        resultData.loadState === 'fail' ||
        resultData.resultdata.length === 0 ||
        !id
    ) {
        return (
            <Banner variant="error" spacing="lg" fullWidth>
                {id
                    ? 'ID not a valid string'
                    : `Data on the following query could not be found: ${id}`}
            </Banner>
        );
    }

    return (
        <div id="resultview" className={styles.searchresult}>
            <div className={styles.result_header}>
                <h3>
                    Search Result for <strong>{id}</strong>
                </h3>
            </div>
            {resultData.resultdata.map((itm: any, index: number) => (
                <div key={index} className={styles.searchitem}>
                    <div>
                        Query type: <strong>{itm.category}</strong>
                    </div>
                    <div>
                        <Longtext text={id} category={itm.category} isLink />
                    </div>
                </div>
            ))}
        </div>
    );
}

export default SearchResult;
