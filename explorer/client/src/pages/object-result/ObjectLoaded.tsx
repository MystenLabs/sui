// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback } from 'react';

import DisplayBox from '../../components/displaybox/DisplayBox';
import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import {
    extractOwnerData,
    trimStdLibPrefix,
    _toSpace,
} from '../../utils/stringUtils';

import styles from './ObjectResult.module.css';

import { isObjectExistsInfo, type GetObjectInfoResponse } from 'sui.js';

import {
    checkIsIDType,
    hasBytesField,
    hasVecField,
    checkVecOfSingleID,
    isSuiPropertyType,
} from '../../utils/typeChecks';

function isString(obj: any): obj is string {
    return typeof obj === 'string';
}

function renderConnectedEntity(
    key: string,
    value: any,
    index1: number
): JSX.Element {
    return (
        <div key={`ConnectedEntity-${index1}`}>
            <div>{_toSpace(key)}</div>
            {hasBytesField(value) && (
                <Longtext text={value.bytes} category="objects" />
            )}
            {hasVecField(value) && (
                <div>
                    {value?.vec.map(
                        (
                            value2: {
                                bytes: string;
                            },
                            index2: number
                        ) => (
                            <Longtext
                                text={value2.bytes}
                                category="objects"
                                key={`ConnectedEntity-${index1}-${index2}`}
                            />
                        )
                    )}
                </div>
            )}
            {checkVecOfSingleID(value) && (
                <div>
                    {value.map(
                        (
                            value2: {
                                bytes: string;
                            },
                            index2: number
                        ) => (
                            <Longtext
                                text={value2.bytes}
                                category="objects"
                                key={`ConnectedEntity-${index1}-${index2}`}
                            />
                        )
                    )}
                </div>
            )}
        </div>
    );
}

function ObjectLoaded({ data }: { data: GetObjectInfoResponse }) {

    const [showDescription, setShowDescription] = useState(true);
    const [showProperties, setShowProperties] = useState(false);
    const [showConnectedEntities, setShowConnectedEntities] = useState(false);

    useEffect(() => {
        setShowDescription(true);
        setShowProperties(true);
        setShowConnectedEntities(true);
    }, [setShowDescription, setShowProperties, setShowConnectedEntities]);

    const clickSetShowDescription = useCallback(
        () => setShowDescription(!showDescription),
        [showDescription]
    );
    const clickSetShowProperties = useCallback(
        () => setShowProperties(!showProperties),
        [showProperties]
    );
    const clickSetShowConnectedEntities = useCallback(
        () => setShowConnectedEntities(!showConnectedEntities),
        [showConnectedEntities]
    );

    if (!isObjectExistsInfo(data.details))
        return <></>;

    const suiObj = data.details.object;
    const suiObjContent = suiObj.contents;
    const objRef = data.details.objectRef;
    const objID = objRef.objectId;

    console.log('suiObj', suiObj, suiObjContent);

    const suiObjName = suiObj['name'];
    const nonNameEntries = Object.entries(suiObjContent).filter(
        ([k, _]) => k !== 'name'
    );

    const ownedObjects: [string, any][] = nonNameEntries.filter(
        ([key, value]) => checkIsIDType(key, value)
    );

    const properties: [string, any][] = nonNameEntries
        .filter(([_, value]) => isSuiPropertyType(value))
        // TODO: 'display' is a object property added during demo, replace with metadata ptr?
        .filter(([key, _]) => key !== 'display');

    console.log('properties', properties);

    return (
        <>
            <div className={styles.resultbox}>
                {(suiObj?.display && isString(suiObjContent.display)) && (
                    // TODO - remove MoveScript tag, don't use Displaybox for Move contracts
                    <DisplayBox display={suiObjContent.display} tag="imageURL" />
                )}
                <div
                    className={`${styles.textbox} ${
                        suiObjContent?.display
                            ? styles.accommodate
                            : styles.noaccommodate
                    }`}
                >
                    {suiObj.name && <h1>{suiObjContent.name}</h1>}{' '}
                    {typeof suiObjName === 'string' && <h1>{suiObjName}</h1>}
                    <h2
                        className={styles.clickableheader}
                        onClick={clickSetShowDescription}
                    >
                        Description {showDescription ? '' : '+'}
                    </h2>
                    {showDescription && (
                        <div
                            className={theme.textresults}
                            id="descriptionResults"
                        >
                            <div>
                                <div>Object ID</div>
                                <div id="objectID">
                                    <Longtext
                                        text={objID}
                                        category="objects"
                                        isLink={false}
                                    />
                                </div>
                            </div>

                            <div>
                                <div>Version</div>
                                <div>{objRef.version}</div>
                            </div>

                            {suiObj.readonly && (
                                <div>
                                    <div>Read Only?</div>
                                    {suiObj.readonly === 'true' ? (
                                        <div
                                            id="readOnlyStatus"
                                            className={styles.immutable}
                                        >
                                            True
                                        </div>
                                    ) : (
                                        <div
                                            id="readOnlyStatus"
                                            className={styles.mutable}
                                        >
                                            False
                                        </div>
                                    )}
                                </div>
                            )}

                            {suiObjContent?.objType && (
                                <div>
                                    <div>Type</div>
                                    <div>{trimStdLibPrefix(suiObjContent.objType)}</div>
                                </div>
                            )}
                            <div>
                                <div>Owner</div>
                                <div id="owner">
                                    <Longtext
                                        text={extractOwnerData(suiObj.owner)}
                                        category="unknown"
                                        isLink={true}
                                    />
                                </div>
                            </div>
                            {suiObj.contract_id && (
                                <div>
                                    <div>Contract ID</div>
                                    <Longtext
                                        text={suiObj.contract_id.bytes}
                                        category="objects"
                                        isLink={true}
                                    />
                                </div>
                            )}

                            {suiObj.ethAddress && (
                                <div>
                                    <div>Ethereum Contract Address</div>
                                    <div>
                                        <Longtext
                                            text={suiObj.ethAddress}
                                            category="ethAddress"
                                            isLink={true}
                                        />
                                    </div>
                                </div>
                            )}
                            {suiObj.ethTokenId && (
                                <div>
                                    <div>Ethereum Token ID</div>
                                    <div>
                                        <Longtext
                                            text={suiObj.ethTokenId}
                                            category="addresses"
                                            isLink={false}
                                        />
                                    </div>
                                </div>
                            )}
                        </div>
                    )}
                    {properties.length > 0 && (
                        <>
                            <h2
                                className={styles.clickableheader}
                                onClick={clickSetShowProperties}
                            >
                                Properties {showProperties ? '' : '+'}
                            </h2>
                            {showProperties && (
                                <div className={styles.propertybox}>
                                    {properties.map(([key, value], index) => (
                                        <div key={`property-${index}`}>
                                            <p>{_toSpace(key)}</p>
                                            <p>{value}</p>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </>
                    )}
                    {ownedObjects.length > 0 && (
                        <>
                            <h2
                                className={styles.clickableheader}
                                onClick={clickSetShowConnectedEntities}
                            >
                                Owned Objects {showConnectedEntities ? '' : '+'}
                            </h2>
                            {showConnectedEntities && (
                                <div className={theme.textresults}>
                                    {ownedObjects.map(([key, value], index1) =>
                                        renderConnectedEntity(
                                            key,
                                            value,
                                            index1
                                        )
                                    )}
                                </div>
                            )}
                        </>
                    )}
                </div>
            </div>
        </>
    );
}

export default ObjectLoaded;
