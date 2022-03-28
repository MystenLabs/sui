import React, { useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { navigateWithUnknown } from '../../utils/searchUtil';
import { findDataFromID } from '../../utils/static/searchUtil';
import { trimStdLibPrefix } from '../../utils/stringUtils';

import styles from './OwnedObjects.module.css';

type resultType = {
    id: string;
    display?: {
        category: string;
        data: string;
    };
}[];

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

function OwnedObjectView({ results }: { results: resultType }) {
    const handlePreviewClick = useCallback(
        (id: string, navigate: Function) => (e: React.MouseEvent) =>
            navigateWithUnknown(id, navigate),
        []
    );
    const navigate = useNavigate();
    return (
        <div>
            {results.map((entryObj, index1) => (
                <div
                    className={styles.objectbox}
                    key={`object-${index1}`}
                    onClick={handlePreviewClick(entryObj.id, navigate)}
                >
                    {'display' in entryObj &&
                    entryObj?.display?.category === 'imageURL' ? (
                        <div className={styles.previewimage}>
                            <img
                                className={styles.imagebox}
                                alt="NFT preview"
                                src={entryObj.display.data}
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
        return <div>Not Supported Yet</div>;
    }
}

export default OwnedObject;
