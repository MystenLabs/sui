// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, getObjectFields, getObjectId } from '@mysten/sui.js';
import BN from 'bn.js';
import React, {
    useCallback,
    useEffect,
    useState,
    useContext,
    createContext,
} from 'react';
import { useNavigate } from 'react-router-dom';

import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { parseImageURL, parseObjectType } from '../../utils/objectUtils';
import { navigateWithUnknown } from '../../utils/searchUtil';
import {
    findDataFromID,
    findOwnedObjectsfromID,
} from '../../utils/static/searchUtil';
import {
    handleCoinType,
    processDisplayValue,
    trimStdLibPrefix,
} from '../../utils/stringUtils';
import DisplayBox from '../displaybox/DisplayBox';
import PaginationWrapper from '../pagination/PaginationWrapper';

import styles from './OwnedObjects.module.css';

type resultType = {
    id: string;
    Type: string;
    _isCoin: boolean;
    Version?: string;
    display?: string;
    balance?: BN;
}[];

const DATATYPE_DEFAULT: resultType = [
    {
        id: '',
        Type: '',
        _isCoin: false,
    },
];

const lastRowHas2Elements = (itemList: any[]): boolean =>
    itemList.length % 3 === 2;

const NoOwnedObjects = () => (
    <div className={styles.fail}>Failed to find Owned Objects</div>
);

const OwnedObject = ({ id, byAddress }: { id: string; byAddress: boolean }) =>
    IS_STATIC_ENV ? (
        <OwnedObjectStatic id={id} />
    ) : (
        <OwnedObjectAPI id={id} byAddress={byAddress} />
    );

const NavigateFunctionContext = createContext<(id: string) => () => void>(
    (id: string) => () => {}
);

function OwnedObjectStatic({ id }: { id: string }) {
    const navigate = useNavigate();

    const navigateFn = useCallback(
        (id: string) => () => navigateWithUnknown(id, navigate),
        [navigate]
    );

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
            };
        });

        return (
            <NavigateFunctionContext.Provider value={navigateFn}>
                <OwnedObjectLayout results={results} />
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
    const navigate = useNavigate();
    const navigateFn = useCallback(
        (id: string) => () => navigateWithUnknown(id, navigate, network),
        [navigate, network]
    );

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
                                    const objType = parseObjectType(resp);
                                    const balanceValue = Coin.getBalance(resp);
                                    return {
                                        id: getObjectId(resp),
                                        Type: objType,
                                        _isCoin: Coin.isCoin(resp),
                                        display: url
                                            ? processDisplayValue(url)
                                            : undefined,
                                        balance: balanceValue,
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

    if (isLoaded)
        return (
            <NavigateFunctionContext.Provider value={navigateFn}>
                <OwnedObjectLayout results={results} />
            </NavigateFunctionContext.Provider>
        );

    return <div className={styles.gray}>loading...</div>;
}

const viewFn = (results: any) => <OwnedObjectView results={results} />;

function OwnedObjectLayout({ results }: { results: resultType }) {
    const coin_results = results.filter(({ _isCoin }) => _isCoin);
    const other_results = results
        .filter(({ _isCoin }) => !_isCoin)
        .sort((a, b) => {
            if (a.Type > b.Type) return 1;
            if (a.Type < b.Type) return -1;
            if (a.Type === b.Type) {
                return a.id <= b.id ? -1 : 1;
            }
            return 0;
        });

    return (
        <div>
            {coin_results.length > 0 && (
                <div>
                    <h2>Coins</h2>
                    <GroupView results={coin_results} />
                </div>
            )}
            {other_results.length > 0 && (
                <div id="NFTSection">
                    <h2>NFTs</h2>
                    <PaginationWrapper
                        results={other_results}
                        viewComponentFn={viewFn}
                    />
                </div>
            )}
        </div>
    );
}

function GroupView({ results }: { results: resultType }) {
    const [subObjs, setSubObjs] = useState(DATATYPE_DEFAULT);

    const [isGroup, setIsGroup] = useState(true);

    const shrinkObjList = useCallback(
        (subObjList) => () => {
            setIsGroup(false);
            setSubObjs(subObjList);
        },
        []
    );

    const goBack = useCallback(() => setIsGroup(true), []);

    const uniqueTypes = Array.from(new Set(results.map(({ Type }) => Type)));

    if (isGroup) {
        return (
            <div id="groupCollection" className={styles.ownedobjects}>
                {uniqueTypes.map((typeV) => {
                    const subObjList = results.filter(
                        ({ Type }) => Type === typeV
                    );
                    return (
                        <div
                            key={typeV}
                            onClick={shrinkObjList(subObjList)}
                            className={styles.objectbox}
                        >
                            <div>
                                <div>
                                    <span>Type</span>
                                    <span>{handleCoinType(typeV)}</span>
                                </div>
                                <div>
                                    <span>Balance</span>
                                    <span>
                                        {subObjList[0]._isCoin &&
                                        subObjList.every(
                                            (el) => el.balance !== undefined
                                        )
                                            ? `${subObjList.reduce(
                                                  (prev, current) =>
                                                      prev.add(
                                                          current.balance!
                                                      ),
                                                  Coin.getZero()
                                              )}`
                                            : ''}
                                    </span>
                                </div>
                            </div>
                        </div>
                    );
                })}
                {lastRowHas2Elements(uniqueTypes) && (
                    <div
                        className={`${styles.objectbox} ${styles.fillerbox}`}
                    />
                )}
            </div>
        );
    } else {
        return (
            <div>
                <div className={styles.paginationheading}>
                    <button onClick={goBack}>&#60; Back</button>
                    <h2>{handleCoinType(subObjs[0].Type)}</h2>
                </div>
                <PaginationWrapper results={subObjs} viewComponentFn={viewFn} />
            </div>
        );
    }
}

function OwnedObjectView({ results }: { results: resultType }) {
    const navigateWithUnknown = useContext(NavigateFunctionContext);
    return (
        <div id="ownedObjects" className={styles.ownedobjects}>
            {results.map((entryObj, index1) => (
                <div
                    className={styles.objectbox}
                    key={`object-${index1}`}
                    onClick={navigateWithUnknown(entryObj.id)}
                >
                    {entryObj.display !== undefined && (
                        <div className={styles.previewimage}>
                            <DisplayBox display={entryObj.display} />
                        </div>
                    )}
                    {Object.entries(entryObj).map(([key, value], index2) => (
                        <div key={`object-${index1}-${index2}`}>
                            {(() => {
                                switch (key) {
                                    case 'display':
                                        break;
                                    case 'Type':
                                        if (entryObj._isCoin) {
                                            break;
                                        } else {
                                            return (
                                                <div>
                                                    <span>{key}</span>
                                                    <span>
                                                        {trimStdLibPrefix(
                                                            value as string
                                                        )}
                                                    </span>
                                                </div>
                                            );
                                        }
                                    default:
                                        if (
                                            key === 'balance' &&
                                            !entryObj._isCoin
                                        )
                                            break;
                                        if (key.startsWith('_')) {
                                            break;
                                        }
                                        return (
                                            <div>
                                                <span>{key}</span>
                                                <span>{String(value)}</span>
                                            </div>
                                        );
                                }
                            })()}
                        </div>
                    ))}
                </div>
            ))}
            {lastRowHas2Elements(results) && (
                <div className={`${styles.objectbox} ${styles.fillerbox}`} />
            )}
        </div>
    );
}

export default OwnedObject;
