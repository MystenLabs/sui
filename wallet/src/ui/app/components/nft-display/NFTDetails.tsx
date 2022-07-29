// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import PageTitle from '_app/shared/page-title';
import Icon, { SuiIcons } from '_components/icon';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './NFTDetials.module.scss';

function NFTDetials() {
    const nfts = useAppSelector(accountNftsSelector);
    const dispatch = useAppDispatch();
    dispatch(setNavVisibility(false));
    return (
        <div className={st.container}>
            <PageTitle title="Stake & Earn" className={st.pageTitle} />
            <BottomMenuLayout>
                <Content>
                    <div className={st.pageDescription}>
                        Staking SUI provides SUI holders with rewards in
                        addition to market price gains.
                    </div>
                </Content>
                <Menu stuckClass={st.shadow}>
                    <Button size="large" mode="neutral" className={st.action}>
                        <Icon
                            icon={SuiIcons.Close}
                            className={st.closeActionIcon}
                        />
                        Cancel
                    </Button>
                    <Button size="large" mode="primary" className={st.action}>
                        Stake Coins
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={st.arrowActionIcon}
                        />
                    </Button>
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}

export default NFTDetials;
