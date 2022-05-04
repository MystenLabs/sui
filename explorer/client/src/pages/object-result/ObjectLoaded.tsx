// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState, useCallback } from 'react';

import DisplayBox from '../../components/displaybox/DisplayBox';
import Longtext from '../../components/longtext/Longtext';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import theme from '../../styles/theme.module.css';
import { type AddressOwner } from '../../utils/api/DefaultRpcClient';
import { parseImageURL } from '../../utils/objectUtils';
import {
    asciiFromNumberBytes,
    trimStdLibPrefix,
} from '../../utils/stringUtils';
import { type DataType } from './ObjectResultType';

import styles from './ObjectResult.module.css';

function ObjectLoaded({ data }: { data: DataType }) {
    // TODO - restore or remove this functionality
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
    const prepLabel = (label: string) => label.split('_').join(' ');
    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    //TODO - a backend convention on how owned objects are labelled and how values are stored
    //This would facilitate refactoring the below and stopping bugs when a variant is missed:
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
        name?: string;
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
            return data['name'] as string;
        } else {
            bytesObj = { bytes: [] };
        }

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

    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        name: processName(data.name),
        tx_digest:
            data.data.tx_digest && typeof data.data.tx_digest === 'object'
                ? toHexString(data.data.tx_digest as number[])
                : data.data.tx_digest,
        owner: processOwner(data.owner),
        url: parseImageURL(data.data),
    };

    //TO DO remove when have distinct name field under Description
    const nameKeyValue = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => /name/i.test(key))
        .map(([_, value]) => value);

    const properties = Object.entries(viewedData.data?.contents)
        //TO DO: remove when have distinct 'name' field in Description
        .filter(([key, _]) => !/name/i.test(key))
        .filter(([_, value]) => checkIsPropertyType(value));

    return (
        <>
            <div className={styles.resultbox}>
                {viewedData.url !== '' && (
                    <div className={styles.display}>
                        <DisplayBox display={viewedData.url} tag="imageURL" />
                    </div>
                )}
                <div
                    className={`${styles.textbox} ${
                        viewedData.url
                            ? styles.accommodate
                            : styles.noaccommodate
                    }`}
                >
                    {data.name && <h1>{data.name}</h1>}{' '}
                    {typeof nameKeyValue[0] === 'string' && (
                        <h1>{nameKeyValue}</h1>
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
                                        text={data.id}
                                        category="objects"
                                        isLink={false}
                                    />
                                </div>
                            </div>

                            <div>
                                <div>Version</div>
                                <div>{data.version}</div>
                            </div>

                            {data.readonly && (
                                <div>
                                    <div>Read Only?</div>
                                    {data.readonly === 'true' ? (
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
                                <div>{prepObjTypeValue(data.objType)}</div>
                            </div>
                            <div>
                                <div>Owner</div>
                                <div id="owner">
                                    <Longtext
                                        text={extractOwnerData(data.owner)}
                                        category="unknown"
                                        // TODO: make this more elegant
                                        isLink={
                                            extractOwnerData(data.owner) !==
                                                'Immutable' &&
                                            extractOwnerData(data.owner) !==
                                                'Shared'
                                        }
                                    />
                                </div>
                            </div>
                            {data.contract_id && (
                                <div>
                                    <div>Contract ID</div>
                                    <Longtext
                                        text={data.contract_id.bytes}
                                        category="objects"
                                        isLink={true}
                                    />
                                </div>
                            )}

                            {data.ethAddress && (
                                <div>
                                    <div>Ethereum Contract Address</div>
                                    <div>
                                        <Longtext
                                            text={data.ethAddress}
                                            category="ethAddress"
                                            isLink={true}
                                        />
                                    </div>
                                </div>
                            )}
                            {data.ethTokenId && (
                                <div>
                                    <div>Ethereum Token ID</div>
                                    <div>
                                        <Longtext
                                            text={data.ethTokenId}
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
                                            <p>{prepLabel(key)}</p>
                                            <p>{value}</p>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </>
                    )}
                    <h2
                        className={styles.clickableheader}
                        onClick={clickSetShowConnectedEntities}
                    >
                        Owned Objects {showConnectedEntities ? '' : '+'}
                    </h2>
                    {showConnectedEntities && <OwnedObjects id={data.id} />}
                </div>
            </div>
        </>
    );
}

export default ObjectLoaded;
