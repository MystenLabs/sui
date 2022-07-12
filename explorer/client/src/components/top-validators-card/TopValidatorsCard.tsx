// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Longtext from '../../components/longtext/Longtext';
import TableCard from '../../components/table/TableCard';
import TabFooter from '../../components/tabs/TabFooter';
import Tabs from '../../components/tabs/Tabs';
import { numberSuffix } from '../../utils/numberUtil';

import styles from './TopValidatorsCard.module.css';

// TODO: Specify the type of the context
// Specify the type of the context
function TopValidatorsCard() {
    // mock validators data
    const validatorsData = [
        {
            validatorName: 'Jump Crypto',
            suiStake: 9_220_000,
            suiStakePercent: '5.2%',
            eporchReward: '38,026',
            position: 1,
        },
        {
            validatorName: 'Blockdaemon',
            suiStake: 8_220_000,
            suiStakePercent: '4.2%',
            eporchReward: '34,100',
            position: 2,
        },
        {
            validatorName: 'Kraken',
            suiStake: 4_650_000,
            suiStakePercent: '2.69%',
            eporchReward: '19,220',
            position: 3,
        },
        {
            validatorName: 'Coinbase',
            suiStake: 4_550_000,
            suiStakePercent: '2.63%',
            eporchReward: '18,806',
            position: 4,
        },
        {
            validatorName: 'a16z',
            suiStake: 2_860_000,
            suiStakePercent: '1.58%',
            eporchReward: '11,821',
            position: 5,
        },
        {
            validatorName: 'Figment',
            suiStake: 2_840_000,
            suiStakePercent: '1.63%',
            eporchReward: '11,736',
            position: 6,
        },
        {
            validatorName: '0x813e...d21f07',
            suiStake: 2_730_000,
            suiStakePercent: '1.58%',
            eporchReward: '11,736',
            position: 7,
        },
        {
            validatorName: '0x813e...d21f07',
            suiStake: 2_730_000,
            suiStakePercent: '1.58%',
            eporchReward: '11,736',
            position: 8,
        },
        {
            validatorName: '0x813e...d21f07',
            suiStake: 2_730_000,
            suiStakePercent: '1.58%',
            eporchReward: '11,736',
            position: 9,
        },
        {
            validatorName: '0x813e...d21f07',
            suiStake: 2_730_000,
            suiStakePercent: '1.58%',
            eporchReward: '11,736',
            position: 10,
        },
    ];
    // map the above data to match the table combine stake and stake percent
    const mockValidatorsData = {
        data: validatorsData.map((validator) => ({
            validatorName: validator.validatorName,
            stake: (
                <div>
                    {' '}
                    {numberSuffix(validator.suiStake)}{' '}
                    <span className={styles.stakepercent}>
                        {' '}
                        {validator.suiStakePercent}
                    </span>
                </div>
            ),
            eporchReward: validator.eporchReward,
            position: validator.position,
        })),
        columns: [
            {
                headerLabel: '#',
                accessorKey: 'position',
            },
            {
                headerLabel: 'Validator',
                accessorKey: 'validatorName',
            },
            {
                headerLabel: 'STAKE',
                accessorKey: 'stake',
            },
            {
                headerLabel: 'LAST EPOCH REWARD',
                accessorKey: 'eporchReward',
            },
        ],
    };

    const tabsFooter = {
        stats: {
            count: 15482,
            stats_text: 'total transactions',
        },
    };

    return (
        <div className={styles.validators}>
            <Tabs selected={0}>
                <div title="Top Validators">
                    <TableCard tabledata={mockValidatorsData} />
                    <TabFooter stats={tabsFooter.stats}>
                        <Longtext
                            text=""
                            category="transactions"
                            isLink={true}
                            isCopyButton={false}
                            showIconButton={true}
                            alttext="More Validators"
                        />
                    </TabFooter>
                </div>
                <div title=""></div>
            </Tabs>
        </div>
    );
}

export default TopValidatorsCard;
