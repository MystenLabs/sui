// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { DefaultRpcClient as rpc } from '../../utils/api/SuiRpcClient';
import { navigateWithUnknown } from '../../utils/searchUtil';
import { findDataFromID } from '../../utils/static/searchUtil';
import { trimStdLibPrefix, processDisplayValue } from '../../utils/stringUtils';
import DisplayBox from '../displaybox/DisplayBox';

import styles from './OwnedObjects.module.css';

type resultType = {
    id: string;
    Type: string;
    Version: string;
    display?: string;
}[];

const DATATYPE_DEFAULT: resultType = [
    {
        id: '',
        Type: '',
        Version: '',
        display: '',
    },
];

function OwnedObjectStatic({ objects }: { objects: string[] }) {
    const results = objects.map((objectId) => {
        const entry = findDataFromID(objectId, undefined);
        return {
            id: entry?.id,
            Type: entry?.objType,
            Version: entry?.version,
            display: entry?.data?.contents?.display,
        };
    });

    return <OwnedObjectView results={results} />;
}

function OwnedObjectAPI({ objects }: { objects: string[] }) {
    const [results, setResults] = useState(DATATYPE_DEFAULT);
    const [isLoaded, setIsLoaded] = useState(false);

    useEffect(() => {
        Promise.all(objects.map((objID) => rpc.getObjectInfo(objID))).then(
            (results) => {
                setResults(
                    results.map(({ id, objType, version, data }) => ({
                        id: id,
                        Type: objType,
                        Version: version,
                        display: processDisplayValue(data.contents?.display),
                    }))
                );
                setIsLoaded(true);
            }
        );
    }, [objects]);

    if (isLoaded) {
        return <OwnedObjectView results={results} />;
    } else {
        return <div />;
    }
}

function OwnedObjectView({ results }: { results: resultType }) {
    const handlePreviewClick = useCallback(
        (id: string, navigate: Function) => (e: React.MouseEvent) =>
            navigateWithUnknown(id, navigate),
        []
    );
    const navigate = useNavigate();
    return (
        <div id="ownedObjects" className={styles.ownedobjects}>
            {results.map((entryObj, index1) => (
                <div
                    className={styles.objectbox}
                    key={`object-${index1}`}
                    onClick={handlePreviewClick(entryObj.id, navigate)}
                >
                    {entryObj.display !== undefined && (
                        <div className={styles.previewimage}>
                            <DisplayBox
                                display={entryObj.display}
                                tag="imageURL"
                            />
                        </div>
                    )}
                    {Object.entries(entryObj).map(([key, value], index2) => (
                        <div key={`object-${index1}-${index2}`}>
                            {(() => {
                                switch (key) {
                                    case 'display':
                                        break;
                                    case 'Type':
                                        return (
                                            <div>
                                                <span>{key}</span>
                                                <span>
                                                    {typeof value === 'string'
                                                        ? trimStdLibPrefix(
                                                              value
                                                          )
                                                        : ''}
                                                </span>
                                            </div>
                                        );
                                    default:
                                        return (
                                            <div>
                                                <span>{key}</span>
                                                <span>{value}</span>
                                            </div>
                                        );
                                }
                            })()}
                        </div>
                    ))}
                </div>
            ))}
        </div>
    );
}

function OwnedObject({ objects }: { objects: string[] }) {
    const [pageIndex, setPageIndex] = useState(0);

    const ITEMS_PER_PAGE = 12;

    const FINAL_PAGE_NO =
        Math.floor(objects.length / ITEMS_PER_PAGE) +
        (objects.length % ITEMS_PER_PAGE !== 0 ? 1 : 0);

    const objectSample = objects.slice(
        pageIndex * ITEMS_PER_PAGE,
        (pageIndex + 1) * ITEMS_PER_PAGE
    );

    const OwnedObjectsRetrieved = (retrieved: string[]) => {
        if (process.env.REACT_APP_DATA === 'static') {
            return <OwnedObjectStatic objects={objectSample} />;
        }
        return <OwnedObjectAPI objects={objectSample} />;
    };

    const handleFirstClick = useCallback(() => setPageIndex(0), []);

    const handleBackClick = useCallback(
        () => pageIndex - 1 >= 0 && setPageIndex(pageIndex - 1),
        [pageIndex]
    );

    const handleNextClick = useCallback(
        () =>
            (pageIndex + 1) * ITEMS_PER_PAGE < objects.length &&
            setPageIndex(pageIndex + 1),
        [pageIndex, objects.length]
    );

    const handleLastClick = useCallback(
        () => setPageIndex(FINAL_PAGE_NO - 1),
        [FINAL_PAGE_NO]
    );

    return (
        <>
            {FINAL_PAGE_NO > 1 && (
                <>
                    <span className={pageIndex === 0 ? styles.gone : ''}>
                        <button
                            className={styles.btncontainer}
                            id="firstBtn"
                            onClick={handleFirstClick}
                            disabled={pageIndex === 0}
                        >
                            First
                        </button>
                        <button
                            className={styles.btncontainer}
                            id="backBtn"
                            onClick={handleBackClick}
                            disabled={pageIndex === 0}
                        >
                            <svg
                                width="12"
                                height="12"
                                xmlns="http://www.w3.org/2000/svg"
                            >
                                <path
                                    d="M 12 12 L 0 6 L 12 0"
                                    fill="transparent"
                                />
                            </svg>
                        </button>
                    </span>

                    <span className={styles.pagenumber}>
                        Page {pageIndex + 1} of {FINAL_PAGE_NO}
                    </span>

                    <span
                        className={
                            pageIndex === FINAL_PAGE_NO - 1 ? styles.gone : ''
                        }
                    >
                        <button
                            id="nextBtn"
                            className={styles.btncontainer}
                            disabled={pageIndex === FINAL_PAGE_NO - 1}
                            onClick={handleNextClick}
                        >
                            <svg
                                width="12"
                                height="12"
                                xmlns="http://www.w3.org/2000/svg"
                            >
                                <path
                                    d="M 0 12 L 12 6 L 0 0"
                                    fill="transparent"
                                />
                            </svg>
                        </button>
                        <button
                            id="lastBtn"
                            disabled={pageIndex === FINAL_PAGE_NO - 1}
                            onClick={handleLastClick}
                            className={styles.btncontainer}
                        >
                            Last
                        </button>
                    </span>
                </>
            )}

            {OwnedObjectsRetrieved(objectSample)}
        </>
    );
}

export default OwnedObject;
