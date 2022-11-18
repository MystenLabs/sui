// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import Longtext from '../../components/longtext/Longtext';
import {
    getValidatorState,
    processValidators,
    ValidatorLoadFail,
    type ValidatorState,
} from '../../pages/validators/Validators';
import { mockState } from '../../pages/validators/mockData';
import { truncate } from '../../utils/stringUtils';

import { useRpc } from '~/hooks/useRpc';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { TabGroup, TabList, Tab, TabPanels, TabPanel } from '~/ui/Tabs';

export const STATE_DEFAULT: ValidatorState = {
    delegation_reward: 0,
    epoch: 0,
    id: { id: '', version: 0 },
    parameters: {
        type: '0x2::sui_system::SystemParameters',
        fields: {
            max_validator_candidate_count: 0,
            min_validator_stake: BigInt(0),
        },
    },
    storage_fund: 0,
    treasury_cap: {
        type: '',
        fields: {},
    },
    validators: {
        type: '0x2::validator_set::ValidatorSet',
        fields: {
            delegation_stake: BigInt(0),
            active_validators: [],
            next_epoch_validators: [],
            pending_removals: '',
            pending_validators: '',
            quorum_stake_threshold: BigInt(0),
            total_validator_stake: BigInt(0),
        },
    },
};

export function TopValidatorsCardStatic() {
    return <TopValidatorsCard state={mockState as ValidatorState} />;
}

export function TopValidatorsCardAPI() {
    const [showObjectState, setObjectState] = useState(STATE_DEFAULT);
    const [loadState, setLoadState] = useState('pending');
    const rpc = useRpc();
    useEffect(() => {
        getValidatorState(rpc)
            .then((objState: ValidatorState) => {
                setObjectState(objState);
                setLoadState('loaded');
            })
            .catch((error: any) => {
                console.log(error);
                setLoadState('fail');
            });
    }, [rpc]);

    if (loadState === 'loaded') {
        return <TopValidatorsCard state={showObjectState as ValidatorState} />;
    }
    if (loadState === 'pending') {
        return (
            <div data-testid="validators-table">
                <TabGroup>
                    <TabList>
                        <Tab>Validators</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div title="Top Validators">
                                <PlaceholderTable
                                    rowCount={3}
                                    rowHeight="13px"
                                    colHeadings={[
                                        'Name',
                                        'Address',
                                        'Pubkey Bytes',
                                    ]}
                                    colWidths={['135px', '220px', '220px']}
                                />
                            </div>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
            </div>
        );
    }
    if (loadState === 'fail') {
        return <ValidatorLoadFail />;
    }

    return <div>Something went wrong</div>;
}

function TopValidatorsCard({ state }: { state: ValidatorState }) {
    const validatorsData = processValidators(
        state.validators.fields.active_validators
    );

    // map the above data to match the table - combine stake and stake percent
    // limit number validators to 10
    // TODO: add sorting
    const tableData = {
        data: validatorsData.splice(0, 10).map((validator) => ({
            name: validator.name,
            address: (
                <Longtext
                    text={validator.address}
                    alttext={truncate(validator.address, 14)}
                    category="addresses"
                    isLink
                />
            ),
            pubkeyBytes: (
                <Longtext
                    text={validator.pubkeyBytes}
                    alttext={truncate(validator.pubkeyBytes, 14)}
                    category="addresses"
                    isLink={false}
                />
            ),
        })),
        columns: [
            {
                headerLabel: 'Name',
                accessorKey: 'name',
            },
            {
                headerLabel: 'Address',
                accessorKey: 'address',
            },
            {
                headerLabel: 'Pubkey Bytes',
                accessorKey: 'pubkeyBytes',
            },
        ],
    };

    return (
        <div data-testid="validators-table">
            <TabGroup>
                <TabList>
                    <Tab>Validators</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        <TableCard
                            data={tableData.data}
                            columns={tableData.columns}
                        />
                        <div className="mt-3">
                            <Link to="/validators">
                                <div className="flex items-center gap-2">
                                    More Validators{' '}
                                    <ArrowRight fill="currentColor" />
                                </div>
                            </Link>
                        </div>
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}

export default TopValidatorsCard;
