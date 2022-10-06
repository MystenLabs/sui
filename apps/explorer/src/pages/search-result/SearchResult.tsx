// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidTransactionDigest, isValidSuiAddress } from '@mysten/sui.js';
import { useEffect, useState, useContext } from 'react';
import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { isGenesisLibAddress } from '../../utils/api/searchUtil';

import styles from './SearchResult.module.css';

type SearchDataType = {
    resultdata: any[];
    loadState?: 'loaded' | 'pending' | 'fail';
};

const initState: SearchDataType = {
    loadState: 'pending',
    resultdata: [],
};

const querySearchParams = async (input: string, network: Network | string) => {
    let searchPromises = [];
    if (isValidTransactionDigest(input)) {
        searchPromises.push(
            rpc(network)
                .getTransactionWithEffects(input)
                .then((data) => [
                    {
                        category: 'transactions',
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
                        category: 'addresses',
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
                        category: 'objects',
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
        return (
            <div className={theme.textresults}>
                <div className={styles.textcenter}>Loading...</div>
            </div>
        );
    }

    if (
        resultData.loadState === 'fail' ||
        resultData.resultdata.length === 0 ||
        !id
    ) {
        return (
            <ErrorResult
                id={id}
                errorMsg={
                    id
                        ? 'ID not a valid string'
                        : 'Data on the following query could not be found'
                }
            />
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
                        <Longtext
                            text={id}
                            category={itm.category}
                            isLink={true}
                        />
                    </div>
                </div>
            ))}
        </div>
    );
}

export default SearchResult;
