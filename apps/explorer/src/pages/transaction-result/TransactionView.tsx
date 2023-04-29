// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusError,
    getExecutionStatusType,
    getTransactionDigest,
    getTransactionKind,
    getTransactionKindName,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';
import clsx from 'clsx';

// import {
//     eventToDisplay,
//     getAddressesLinks,
// } from '../../components/events/eventDisplay';

import { Signatures } from './Signatures';

import styles from './TransactionResult.module.css';

import { useBreakpoint } from '~/hooks/useBreakpoint';
import { TransactionData } from '~/pages/transaction-result/TransactionData';
import { TransactionSummary } from '~/pages/transaction-result/transaction-summary';
import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';
import { SplitPanes } from '~/ui/SplitPanes';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

export function TransactionView({
    transaction,
}: {
    transaction: SuiTransactionBlockResponse;
}) {
    const isMediumOrAbove = useBreakpoint('md');

    // const txKindData = formatByTransactionKind(txKindName, txnDetails, sender);
    // const txEventData = transaction.events?.map(eventToDisplay);

    // MUSTFIX(chris): re-enable event display
    // let eventTitles: [string, string][] = [];
    // const txEventDisplay = txEventData?.map((ed, index) => {
    //     if (!ed) return <div />;

    //     let key = ed.top.title + index;
    //     eventTitles.push([ed.top.title, key]);
    //     return (
    //         <div className={styles.txgridcomponent} key={key}>
    //             <ItemView data={ed.top as TxItemView} />
    //             {ed.fields && <ItemView data={ed.fields as TxItemView} />}
    //         </div>
    //     );
    // });

    // let eventTitlesDisplay = eventTitles.map(([title, key]) => (
    //     <div key={key} className={styles.eventtitle}>
    //         {title}
    //     </div>
    // ));

    // MUSTFIX(chris): re-enable event display
    // const hasEvents = txEventData && txEventData.length > 0;
    const hasEvents = false;

    const txError = getExecutionStatusError(transaction);

    const transactionKindName = getTransactionKindName(
        getTransactionKind(transaction)!
    );

    const isProgrammableTransaction =
        transactionKindName === 'ProgrammableTransaction';

    const leftPane = {
        panel: (
            <div className="h-full overflow-y-scroll rounded-2xl border border-transparent bg-gray-40 p-6 md:h-screen md:p-10">
                <TabGroup size="lg">
                    <TabList>
                        <Tab>Summary</Tab>
                        {hasEvents && <Tab>Events</Tab>}
                        {isProgrammableTransaction && <Tab>Signatures</Tab>}
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div className="mt-10">
                                <TransactionSummary transaction={transaction} />
                            </div>
                        </TabPanel>
                        {/* {hasEvents && (
                        <TabPanel>
                            <div className={styles.txevents}>
                                <div className={styles.txeventsleft}>
                                    {eventTitlesDisplay}
                                </div>
                                <div className={styles.txeventsright}>
                                    {txEventDisplay}
                                </div>
                            </div>
                        </TabPanel>
                    )} */}
                        <TabPanel>
                            <Signatures transaction={transaction} />
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
            </div>
        ),
        minSize: 35,
        collapsible: true,
        collapsibleButton: true,
        noHoverHidden: isMediumOrAbove,
    };

    const rightPane = {
        panel: (
            <div className="h-full w-full overflow-y-scroll md:overflow-y-hidden">
                <TransactionData transaction={transaction} />
            </div>
        ),
        minSize: 40,
        defaultSize: isProgrammableTransaction ? 65 : 50,
    };

    return (
        <div className={clsx(styles.txdetailsbg)}>
            <div className="mb-10">
                <PageHeader
                    type="Transaction"
                    title={getTransactionDigest(transaction)}
                    subtitle={
                        !isProgrammableTransaction
                            ? transactionKindName
                            : undefined
                    }
                    status={getExecutionStatusType(transaction)}
                />
                {txError && (
                    <div className="mt-2">
                        <Banner variant="error">{txError}</Banner>
                    </div>
                )}
            </div>
            <div className="h-verticalListLong md:h-full">
                <SplitPanes
                    splitPanels={[leftPane, rightPane]}
                    direction={isMediumOrAbove ? 'horizontal' : 'vertical'}
                />
            </div>
        </div>
    );
}
