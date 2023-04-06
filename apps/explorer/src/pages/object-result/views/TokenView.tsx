// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight12 } from '@mysten/icons';
import {
    normalizeSuiAddress,
    type SuiObjectResponse,
    getObjectDisplay,
    getObjectOwner,
    getObjectId,
    getObjectVersion,
    getObjectPreviousTransactionDigest,
    getObjectFields,
    getMovePackageContent,
} from '@mysten/sui.js';
import { useState, useEffect, useCallback } from 'react';

import DisplayBox from '../../../components/displaybox/DisplayBox';
import ModulesWrapper from '../../../components/module/ModulesWrapper';
import {
    parseImageURL,
    checkIsPropertyType,
    extractName,
} from '../../../utils/objectUtils';
import { trimStdLibPrefix, genFileTypeMsg } from '../../../utils/stringUtils';
import { LinkOrTextDescriptionItem } from '../LinkOrTextDescriptionItem';

import styles from './ObjectView.module.css';

import { DynamicFieldsCard } from '~/components/ownedobjects/views/DynamicFieldsCard';
import { TransactionsForAddress } from '~/components/transactions/TransactionsForAddress';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { Heading } from '~/ui/Heading';
import { AddressLink, ObjectLink, TransactionLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import { parseObjectType } from '~/utils/objectUtils';

export function TokenView({ data }: { data: SuiObjectResponse }) {
    const display = getObjectDisplay(data);
    const imgUrl = parseImageURL(display.data);
    const objOwner = getObjectOwner(data);
    const name = extractName(display.data);

    const objectId = getObjectId(data);
    const objectType = parseObjectType(data);
    const contents = getObjectFields(data) ?? getMovePackageContent(data)!;

    const [fileType, setFileType] = useState<undefined | string>(undefined);

    const properties = Object.entries(contents).filter(
        ([key, value]) => key !== 'name' && checkIsPropertyType(value)
    );

    const structProperties = Object.entries(contents).filter(
        ([key, value]) => typeof value == 'object' && key !== 'id'
    );
    let structPropertiesDisplay: any[] = [];
    if (structProperties.length > 0) {
        structPropertiesDisplay = Object.values(structProperties).map(
            ([x, y]) => [x, JSON.stringify(y, null, 2)]
        );
    }

    useEffect(() => {
        const controller = new AbortController();
        genFileTypeMsg(imgUrl, controller.signal)
            .then((result) => setFileType(result))
            .catch((err) => console.log(err));

        return () => {
            controller.abort();
        };
    }, [imgUrl]);

    const [isImageFullScreen, setImageFullScreen] = useState<boolean>(false);

    const handlePreviewClick = useCallback(() => {
        setImageFullScreen(true);
    }, []);

    const genhref = (objType: string) => {
        const metadataarr = objType.split('::');
        const address = normalizeSuiAddress(metadataarr[0]);
        return `/object/${address}?module=${metadataarr[1]}`;
    };

    return (
        <div className="flex flex-col flex-nowrap gap-14">
            <TabGroup size="lg">
                <TabList>
                    <Tab>Details</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel noGap>
                        <div className="flex flex-col md:flex-row md:divide-x md:divide-gray-45">
                            <div className="flex-1 divide-y divide-gray-45 pb-6 md:basis-2/3 md:pb-0">
                                <div className="pb-7 pr-10 pt-4">
                                    <DescriptionList>
                                        {objOwner ? (
                                            <DescriptionItem
                                                title="Owner"
                                                data-testid="owner"
                                            >
                                                {objOwner === 'Immutable' ? (
                                                    'Immutable'
                                                ) : 'Shared' in objOwner ? (
                                                    'Shared'
                                                ) : 'ObjectOwner' in
                                                  objOwner ? (
                                                    <ObjectLink
                                                        objectId={
                                                            objOwner.ObjectOwner
                                                        }
                                                    />
                                                ) : (
                                                    <AddressLink
                                                        address={
                                                            objOwner.AddressOwner
                                                        }
                                                    />
                                                )}
                                            </DescriptionItem>
                                        ) : null}
                                        <DescriptionItem title="Object ID">
                                            <ObjectLink
                                                objectId={getObjectId(data)}
                                                noTruncate
                                            />
                                        </DescriptionItem>
                                        <DescriptionItem title="Type">
                                            {/* TODO: Support module links on `ObjectLink` */}
                                            <Link
                                                to={genhref(objectType)}
                                                variant="mono"
                                            >
                                                {trimStdLibPrefix(objectType)}
                                            </Link>
                                        </DescriptionItem>
                                        <DescriptionItem title="Version">
                                            <Text
                                                variant="body/medium"
                                                color="steel-darker"
                                            >
                                                {getObjectVersion(data)}
                                            </Text>
                                        </DescriptionItem>
                                        <DescriptionItem title="Last Transaction Block Digest">
                                            <TransactionLink
                                                digest={
                                                    getObjectPreviousTransactionDigest(
                                                        data
                                                    )!
                                                }
                                                noTruncate
                                            />
                                        </DescriptionItem>
                                    </DescriptionList>
                                </div>
                                {display ? (
                                    <div className="pr-10 pt-2 md:pt-2.5">
                                        <DescriptionList>
                                            <LinkOrTextDescriptionItem
                                                title="Name"
                                                value={name}
                                            />
                                            <LinkOrTextDescriptionItem
                                                title="Description"
                                                value={display.description}
                                            />
                                            <LinkOrTextDescriptionItem
                                                title="Creator"
                                                value={display.creator}
                                                parseUrl
                                            />
                                            <LinkOrTextDescriptionItem
                                                title="Link"
                                                value={display.link}
                                                parseUrl
                                            />
                                            <LinkOrTextDescriptionItem
                                                title="Website"
                                                value={display.project_url}
                                                parseUrl
                                            />
                                        </DescriptionList>
                                    </div>
                                ) : null}
                            </div>
                            {imgUrl !== '' && (
                                <div className="border-0 border-t border-solid border-gray-45 pt-6 md:basis-1/3 md:border-t-0 md:pl-10">
                                    <div className="flex flex-row flex-nowrap gap-5">
                                        <div className="flex w-40 justify-center md:w-50">
                                            <DisplayBox
                                                display={imgUrl}
                                                caption={
                                                    name ||
                                                    trimStdLibPrefix(objectType)
                                                }
                                                fileInfo={fileType}
                                                modalImage={[
                                                    isImageFullScreen,
                                                    setImageFullScreen,
                                                ]}
                                            />
                                        </div>
                                        <div className="flex flex-col justify-center gap-2.5">
                                            {name && (
                                                <Heading
                                                    variant="heading4/semibold"
                                                    color="gray-90"
                                                >
                                                    {name}
                                                </Heading>
                                            )}
                                            {fileType && (
                                                <Text
                                                    variant="bodySmall/medium"
                                                    color="steel-darker"
                                                >
                                                    {fileType}
                                                </Text>
                                            )}
                                            <div>
                                                <Link
                                                    size="captionSmall"
                                                    uppercase
                                                    onClick={handlePreviewClick}
                                                    after={
                                                        <ArrowRight12 className="-rotate-45" />
                                                    }
                                                >
                                                    Preview
                                                </Link>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            )}
                        </div>
                    </TabPanel>
                </TabPanels>
            </TabGroup>

            {properties.length > 0 && (
                <div>
                    <h2 className={styles.header}>Properties</h2>
                    <table className={styles.properties}>
                        <tbody>
                            {properties.map(([key, value], index) => (
                                <tr key={index}>
                                    <td>{key}</td>
                                    <td>
                                        {/* TODO: Use normalized module to determine this display. */}
                                        {typeof value === 'string' &&
                                        (value.startsWith('http://') ||
                                            value.startsWith('https://')) ? (
                                            <Link
                                                href={value}
                                                target="_blank"
                                                rel="noopener noreferrer"
                                            >
                                                {value}
                                            </Link>
                                        ) : (
                                            value
                                        )}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            )}
            {structProperties.length > 0 && (
                <ModulesWrapper
                    data={{
                        title: '',
                        content: structPropertiesDisplay,
                    }}
                />
            )}

            <DynamicFieldsCard id={objectId} />

            <div>
                <h2 className={styles.header}>Transaction Blocks</h2>
                <TransactionsForAddress address={objectId} type="object" />
            </div>
        </div>
    );
}
