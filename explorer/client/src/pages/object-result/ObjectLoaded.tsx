// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback } from 'react';

import DisplayBox from '../../components/displaybox/DisplayBox';
import Longtext from '../../components/longtext/Longtext';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import theme from '../../styles/theme.module.css';
<<<<<<< HEAD
import { type AddressOwner } from '../../utils/api/SuiRpcClient';
=======
>>>>>>> explorer-jsonrpc
import {
    extractOwnerData, trimStdLibPrefix, _toSpace,
} from '../../utils/stringUtils';
<<<<<<< HEAD
import { type DataType } from './ObjectResultType';
=======
import DisplayBox from './DisplayBox';
>>>>>>> explorer-jsonrpc

import styles from './ObjectResult.module.css';
import { GetObjectInfoResponse } from 'sui.js';
import { checkIsIDType, hasBytesField, hasVecField, checkVecOfSingleID, isSuiPropertyType } from '../../utils/typeChecks';


function renderConnectedEntity(key: string, value: any, index1: number): JSX.Element {
    return (
    <div key={`ConnectedEntity-${index1}`}>
        <div>{_toSpace(key)}</div>
        {hasBytesField(value) && (
            <Longtext
                text={value.bytes}
                category="objects"
            />
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
                            text={
                                value2.bytes
                            }
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
                            text={
                                value2.bytes
                            }
                            category="objects"
                            key={`ConnectedEntity-${index1}-${index2}`}
                        />
                    )
                )}
            </div>
        )}
    </div>
    )
}

function ObjectLoaded({ data }: { data: GetObjectInfoResponse }) {

    // TODO - remove all '@ts-ignore' when type defs are fixed
    //@ts-ignore
    const suiObj = data.details.object;
    //@ts-ignore
    const objRef = data.details.objectRef;
    const objID = objRef.objectId;

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
<<<<<<< HEAD
    const prepLabel = (label: string) => label.split('_').join(' ');
    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    //TODO - a backend convention on how owned objects are labelled and how values are stored
    //This would facilitate refactoring the below and stopping bugs when a variant is missed:
    const checkIsIDType = (key: string, value: any) =>
        /owned/.test(key) ||
        (/_id/.test(key) && value?.bytes) ||
        value?.vec ||
        key === 'objects';
    const checkVecOfSingleID = (value: any) =>
        Array.isArray(value) && value.length > 0 && value[0]?.bytes;
    const addrOwnerPattern = /^AddressOwner\(k#/;
    const endParensPattern = /\){1}$/;

    //TODO - improve move code handling:
    // const isMoveVecType = (value: { vec?: [] }) => Array.isArray(value?.vec);
    // TODO - merge / replace with other version of same thing
    const stdLibRe = /0x2::/;
    const prepObjTypeValue = (typeString: string) =>
        typeString.replace(stdLibRe, '');

    const extractOwnerData = (owner: string | AddressOwner): string => {
        switch (typeof owner) {
            case 'string':
                if (addrOwnerPattern.test(owner)) {
                    let ownerId = getAddressOwnerId(owner);
                    return ownerId ? ownerId : '';
                }
                const singleOwnerPattern = /SingleOwner\(k#(.*)\)/;
                const result = singleOwnerPattern.exec(owner);
                return result ? result[1] : '';
            case 'object':
                if ('AddressOwner' in owner) {
                    let ownerId = extractAddressOwner(owner.AddressOwner);
                    return ownerId ? ownerId : '';
                }
                return '';
            default:
                return '';
        }
    };
    const getAddressOwnerId = (addrOwner: string): string | null => {
        if (
            !addrOwnerPattern.test(addrOwner) ||
            !endParensPattern.test(addrOwner)
        )
            return null;

        let str = addrOwner.replace(addrOwnerPattern, '');
        return str.replace(endParensPattern, '');
    };

    const extractAddressOwner = (addrOwner: number[]): string | null => {
        if (addrOwner.length !== 20) {
            console.log('address owner byte length must be 20');
            return null;
        }

        return asciiFromNumberBytes(addrOwner);
    };
    type SuiIdBytes = { bytes: number[] };

    function handleSpecialDemoNameArrays(data: {
        name?: SuiIdBytes | string;
        player_name?: SuiIdBytes | string;
        monster_name?: SuiIdBytes | string;
        farm_name?: SuiIdBytes | string;
    }): string {
        let bytesObj: SuiIdBytes = { bytes: [] };

        if ('player_name' in data) {
            bytesObj = data.player_name as SuiIdBytes;
            const ascii = asciiFromNumberBytes(bytesObj.bytes);
            delete data.player_name;
            return ascii;
        } else if ('monster_name' in data) {
            bytesObj = data.monster_name as SuiIdBytes;
            const ascii = asciiFromNumberBytes(bytesObj.bytes);
            delete data.monster_name;
            return ascii;
        } else if ('farm_name' in data) {
            bytesObj = data.farm_name as SuiIdBytes;
            const ascii = asciiFromNumberBytes(bytesObj.bytes);
            delete data.farm_name;
            return ascii;
        } else if ('name' in data) {
            bytesObj = data.name as SuiIdBytes;
            return asciiFromNumberBytes(bytesObj.bytes);
        } else bytesObj = { bytes: [] };

        return asciiFromNumberBytes(bytesObj.bytes);
    }

    function toHexString(byteArray: number[]): string {
        return (
            '0x' +
            Array.prototype.map
                .call(byteArray, (byte) => {
                    return ('0' + (byte & 0xff).toString(16)).slice(-2);
                })
                .join('')
        );
    }

    function processName(name: string | undefined) {
        // hardcode a friendly name for gas for now
        const gasTokenTypeStr = 'Coin::Coin<0x2::GAS::GAS>';
        const gasTokenId = '0000000000000000000000000000000000000003';
        if (data.objType === gasTokenTypeStr && data.id === gasTokenId)
            return 'GAS';

        if (!name) {
            return handleSpecialDemoNameArrays(data.data.contents);
        }
    }

    function processOwner(owner: any) {
        if (typeof owner === 'object' && 'AddressOwner' in owner) {
            return toHexString(owner.AddressOwner);
        }

        return owner;
    }
=======
>>>>>>> explorer-jsonrpc


    const suiObjName = suiObj['name'];
    const nonNameEntries = Object.entries(suiObj).filter(([k, _]) => k === 'name');

<<<<<<< HEAD
    const ownedObjects = Object.entries(viewedData.data?.contents)
        .filter(([key, value]) => checkIsIDType(key, value))
        .map(([key, value]) => {
            if (value?.bytes !== undefined) return [key, [value.bytes]];

            if (checkVecOfSingleID(value.vec))
                return [
                    key,
                    value.vec.map((value2: { bytes: string }) => value2?.bytes),
                ];

            if (checkVecOfSingleID(value))
                return [
                    key,
                    value.map((value2: { bytes: string }) => value2?.bytes),
                ];

            return [key, []];
        });
=======
    const ownedObjects: [string, any][] = nonNameEntries.filter(
        ([key, value]) => checkIsIDType(key, value)
    );
>>>>>>> explorer-jsonrpc

    const properties: [string, any][] = nonNameEntries
        .filter(([_, value]) => isSuiPropertyType(value))
        // TODO: 'display' is a object property added during demo, replace with metadata ptr?
        .filter(([key, _]) => key !== 'display');

    return (
        <>
            <div className={styles.resultbox}>
<<<<<<< HEAD
                {viewedData.data?.contents?.display && (
                    <div className={styles.display}>
                        <DisplayBox
                            display={viewedData.data.contents.display}
                            tag="imageURL"
                        />
                    </div>
=======
                {suiObj?.display && (
                    <DisplayBox data={data} />
>>>>>>> explorer-jsonrpc
                )}
                <div
                    className={`${styles.textbox} ${
                        suiObj?.display
                            ? styles.accommodate
                            : styles.noaccommodate
                    }`}
                >
                    {suiObj.name && <h1>{suiObj.name}</h1>} {' '}
                    {typeof suiObjName === 'string' && (
                        <h1>{suiObjName}</h1>
                    )}
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

                            <div>
                                <div>Type</div>
                                <div>{trimStdLibPrefix(suiObj.objType)}</div>
                            </div>
                            <div>
                                <div>Owner</div>
<<<<<<< HEAD
                                <div id="owner">
                                    <Longtext
                                        text={extractOwnerData(data.owner)}
                                        category="unknown"
                                        isLink={true}
                                    />
                                </div>
=======
                                <Longtext
                                    text={extractOwnerData(suiObj.owner)}
                                    category="unknown"
                                    isLink={true}
                                />
>>>>>>> explorer-jsonrpc
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
                            {showProperties &&  (
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
                                    {ownedObjects.map(
                                        ([key, value], index1) => (
<<<<<<< HEAD
                                            <div
                                                key={`ConnectedEntity-${index1}`}
                                            >
                                                <div>{prepLabel(key)}</div>
                                                <OwnedObjects objects={value} />
                                            </div>
=======
                                            renderConnectedEntity(key, value, index1)
>>>>>>> explorer-jsonrpc
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
