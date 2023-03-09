// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../../components/error-boundary/ErrorBoundary';
import PkgModulesWrapper from '../../../components/module/PkgModulesWrapper';
import TxForID from '../../../components/transaction-card/TxForID';
import { getOwnerStr } from '../../../utils/objectUtils';
import { trimStdLibPrefix } from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';

import { DescriptionItem, DescriptionList } from '~/ui/DescriptionList';
import { Heading } from '~/ui/Heading';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';

function PkgView({ data }: { data: DataType }) {
    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
    };

    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    const properties = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key !== 'name')
        .filter(([_, value]) => checkIsPropertyType(value));

    return (
        <div className="flex flex-col gap-14">
            <TabGroup size="lg">
                <TabList>
                    <Tab>Details</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        <DescriptionList>
                            <DescriptionItem title="Object ID">
                                <ObjectLink objectId={viewedData.id} />
                            </DescriptionItem>
                            <DescriptionItem title="Object Version">
                                <Text color="steel-darker" variant="p1/medium">
                                    {viewedData.version}
                                </Text>
                            </DescriptionItem>
                            {/* todo: enable this when we have package version history
                            <DescriptionItem title="All Package Versions">
                                <VersionsDisclosure />
                            </DescriptionItem> */}
                            {viewedData?.publisherAddress && (
                                <DescriptionItem title="Publisher">
                                    <AddressLink
                                        address={viewedData.publisherAddress}
                                        noTruncate
                                    />
                                </DescriptionItem>
                            )}
                        </DescriptionList>
                    </TabPanel>
                </TabPanels>
            </TabGroup>

            <div className="flex flex-col gap-2">
                <Heading color="steel-darker" variant="heading4/semibold">
                    Modules
                </Heading>
                <ErrorBoundary>
                    <PkgModulesWrapper id={data.id} modules={properties} />
                </ErrorBoundary>
            </div>
            <div className="flex flex-col gap-2">
                <Heading variant="heading2/semibold" color="steel-darker">
                    Transactions
                </Heading>
                <ErrorBoundary>
                    <TxForID id={viewedData.id} category="object" />
                </ErrorBoundary>
            </div>
        </div>
    );
}

export default PkgView;
