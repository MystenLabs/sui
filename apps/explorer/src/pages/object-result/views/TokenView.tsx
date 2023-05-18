// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, CoinFormat } from '@mysten/core';
import {
    normalizeSuiAddress,
    type SuiObjectResponse,
    getObjectDisplay,
    getObjectOwner,
    getObjectId,
    getObjectVersion,
    getObjectPreviousTransactionDigest,
    getSuiObjectData,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { useState, useEffect } from 'react';

import { trimStdLibPrefix, genFileTypeMsg } from '../../../utils/stringUtils';
import { LinkOrTextDescriptionItem } from '../LinkOrTextDescriptionItem';

import { DynamicFieldsCard } from '~/components/Object/DynamicFieldsCard';
import { ObjectFieldsCard } from '~/components/Object/ObjectFieldsCard';
import TransactionBlocksForAddress from '~/components/TransactionBlocksForAddress/TransactionBlocksForAddress';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { AddressLink, ObjectLink, TransactionLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { ObjectDetails } from '~/ui/ObjectDetails';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import {
    extractName,
    parseImageURL,
    parseObjectType,
} from '~/utils/objectUtils';

export function TokenView({ data }: { data: SuiObjectResponse }) {
    const display = getObjectDisplay(data)?.data;
    const imgUrl = parseImageURL(display);
    const objOwner = getObjectOwner(data);
    const name = extractName(display);
    const objectId = getObjectId(data);
    const objectType = parseObjectType(data);
    const storageRebate = getSuiObjectData(data)?.storageRebate;
    const [storageRebateFormatted, symbol] = useFormatCoin(
        storageRebate,
        SUI_TYPE_ARG,
        CoinFormat.FULL
    );

    const [fileType, setFileType] = useState<undefined | string>(undefined);

    useEffect(() => {
        const controller = new AbortController();
        genFileTypeMsg(imgUrl, controller.signal)
            .then((result) => setFileType(result))
            .catch((err) => console.log(err));

        return () => {
            controller.abort();
        };
    }, [imgUrl]);

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
                            <div className="flex-1 divide-y divide-gray-45 pb-6 md:basis-2/3 md:pb-0 md:pr-10">
                                <div className="py-4 pb-7">
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
                                    <div className="py-4 pb-7">
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
                                {storageRebate && (
                                    <div className="py-4 pb-7">
                                        <DescriptionList>
                                            <DescriptionItem title="Storage Rebate">
                                                <div className="leading-1 flex items-end gap-0.5">
                                                    <Text
                                                        variant="body/medium"
                                                        color="steel-darker"
                                                    >
                                                        {storageRebateFormatted}
                                                    </Text>
                                                    <Text
                                                        variant="captionSmall/normal"
                                                        color="steel"
                                                    >
                                                        {symbol}
                                                    </Text>
                                                </div>
                                            </DescriptionItem>
                                        </DescriptionList>
                                    </div>
                                )}
                            </div>
                            {imgUrl !== '' && (
                                <div className="min-w-0 border-0 border-t border-solid border-gray-45 pt-6 md:basis-1/3 md:border-t-0 md:pl-10">
                                    <div className="flex flex-row flex-nowrap gap-5">
                                        <ObjectDetails
                                            image={imgUrl}
                                            name={
                                                name ||
                                                display?.description ||
                                                trimStdLibPrefix(objectType)
                                            }
                                            type={fileType ?? ''}
                                            variant="large"
                                        />
                                    </div>
                                </div>
                            )}
                        </div>
                    </TabPanel>
                </TabPanels>
            </TabGroup>
            <ObjectFieldsCard id={objectId} />
            <DynamicFieldsCard id={objectId} />
            <TransactionBlocksForAddress address={objectId} isObject />
        </div>
    );
}
