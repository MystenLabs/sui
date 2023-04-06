// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import PaginationLogic from '../../pagination/PaginationLogic';
import { type DataType, ITEMS_PER_PAGE } from '../OwnedObjectConstants';
import OwnedCoinView from './OwnedCoinView';
import OwnedNFTView from './OwnedNFTView';

import { Heading } from '~/ui/Heading';

const viewFn = (results: any) => <OwnedNFTView results={results} />;

export default function OwnedObjectView({ results }: { results: DataType }) {
    const coin_results = results.filter(({ _isCoin }) => _isCoin);
    const other_results = results
        .filter(({ _isCoin }) => !_isCoin)
        .sort((a, b) => {
            if (a.Type > b.Type) return 1;
            if (a.Type < b.Type) return -1;
            if (a.Type === b.Type) {
                return a.id <= b.id ? -1 : 1;
            }
            return 0;
        });

    const nftFooter = {
        stats: {
            count: other_results.length,
            stats_text: 'Total NFTs',
        },
    };
    return (
        <div className="grid w-full grid-cols-1 divide-x-0 divide-gray-45 md:grid-cols-2 md:divide-x">
            {coin_results.length > 0 && (
                <div className="space-y-5 pr-0 pt-5 xl:pr-10">
                    <Heading color="gray-90" variant="heading4/semibold">
                        Coins
                    </Heading>
                    <OwnedCoinView results={coin_results} />
                </div>
            )}

            <div className="pl-0 md:pl-10">
                {other_results.length > 0 && (
                    <div className="py-5" data-testid="owned-nfts">
                        <Heading color="gray-90" variant="heading4/semibold">
                            NFTs
                        </Heading>
                    </div>
                )}

                <PaginationLogic
                    results={other_results}
                    viewComponentFn={viewFn}
                    itemsPerPage={ITEMS_PER_PAGE}
                    stats={nftFooter.stats}
                />
            </div>
        </div>
    );
}
