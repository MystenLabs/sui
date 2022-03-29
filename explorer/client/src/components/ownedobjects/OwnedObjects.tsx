// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { DefaultRpcClient as rpc } from '../../utils/internetapi/SuiRpcClient';
import { navigateWithUnknown } from '../../utils/searchUtil';
import { findDataFromID } from '../../utils/static/searchUtil';
import { trimStdLibPrefix, processDisplayValue } from '../../utils/stringUtils';
import DisplayBox from '../displaybox/DisplayBox';

import styles from './OwnedObjects.module.css';

type resultType = {
    id: string;
    Type: string;
    display?: string;
}[];

const DATATYPE_DEFAULT: resultType = [
    {
        id: '',
        Type: '',
        display: '',
    },
];

function OwnedObjectStatic({ objects }: { objects: string[] }) {
    const results = objects.map((objectId) => {
        const entry = findDataFromID(objectId, undefined);
        return {
            id: entry?.id,
            Type: entry?.objType,
            display: entry?.data?.contents?.display,
        };
    });

    return <OwnedObjectView results={results} />;
}

function OwnecObjectInternetAPI({ objects }: { objects: string[] }) {
    const [results, setResults] = useState(DATATYPE_DEFAULT);
    const [isLoaded, setIsLoaded] = useState(false);

    useEffect(() => {
        Promise.all(objects.map((objID) => rpc.getObjectInfo(objID))).then(
            (results) => {
                setResults(
                    results.map(({ id, objType, data }) => ({
                        id: id,
                        Type: objType,
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
        <div id="ownedObjects">
            {results.map((entryObj, index1) => (
                <div
                    className={styles.objectbox}
                    key={`object-${index1}`}
                    onClick={handlePreviewClick(entryObj.id, navigate)}
                >
                    {entryObj.display !== undefined ? (
                        <div className={styles.previewimage}>
                            <DisplayBox
                                display={entryObj.display}
                                tag="imageURL"
                            />
                        </div>
                    ) : (
                        <div className={styles.previewimage} />
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
    if (process.env.REACT_APP_DATA === 'static') {
        return <OwnedObjectStatic objects={objects} />;
    } else {
        return <OwnecObjectInternetAPI objects={objects} />;
    }
}

export default OwnedObject;
