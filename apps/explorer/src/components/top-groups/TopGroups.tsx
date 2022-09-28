// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Longtext from '../../components/longtext/Longtext';
import TableCard from '../../components/table/TableCard';
import TabFooter from '../../components/tabs/TabFooter';
import Tabs from '../../components/tabs/Tabs';
import { numberSuffix } from '../../utils/numberUtil';

import styles from './TopGroups.module.css';

// TODO: Specify the type of the context
// Specify the type of the context
function TopGroupsCard() {
    // mock validators data
    const validatorsData = [
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
        {
            name: 'BoredApe',
            volume: '3,299',
            floorprice: '1,672',
            transaction: numberSuffix(17_220_000),
            position: 1,
        },
    ];
    const mockValidatorsData = {
        data: validatorsData,
        columns: [
            {
                headerLabel: '#',
                accessorKey: 'position',
            },
            {
                headerLabel: 'NAME',
                accessorKey: 'name',
            },
            {
                headerLabel: 'FLOOR PRICE',
                accessorKey: 'floorprice',
            },
            {
                headerLabel: 'TRANSACTIONS',
                accessorKey: 'transaction',
            },
        ],
    };
    const defaultActiveTab = 1;
    const tabsFooter = {
        stats: {
            count: 326,
            stats_text: 'Collections',
        },
    };
    // Mork data
    return (
        <div className={styles.validators}>
            <Tabs selected={defaultActiveTab}>
                <div title="Top Modules"></div>
                <div title="Top NFT Collections">
                    <TableCard tabledata={mockValidatorsData} />
                    <TabFooter stats={tabsFooter.stats}>
                        <Longtext
                            text=""
                            category="transactions"
                            isLink={true}
                            showIconButton={true}
                            alttext="More NFT Collections"
                        />
                    </TabFooter>
                </div>
                <div title="Top Addresses">Top Address Component</div>
            </Tabs>
        </div>
    );
}

export default TopGroupsCard;
