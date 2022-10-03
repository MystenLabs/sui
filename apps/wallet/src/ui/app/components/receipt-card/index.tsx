// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useIntl } from 'react-intl';

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import { formatDate } from '_helpers';
import { useFileExtentionType } from '_hooks';
import { GAS_SYMBOL } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { TxResultState } from '_redux/slices/txresults';

import st from './ReceiptCard.module.scss';

type TxResponseProps = {
    txDigest: TxResultState;
    tranferType?: 'nft' | 'coin' | null;
};

function ReceiptCard({ tranferType, txDigest }: TxResponseProps) {
    const TxIcon = txDigest.isSender ? SuiIcons.ArrowLeft : SuiIcons.Buy;
    const iconClassName = txDigest.isSender
        ? cl(st.arrowActionIcon, st.angledArrow)
        : cl(st.arrowActionIcon, st.buyIcon);

    const intl = useIntl();

    const imgUrl = txDigest?.url
        ? txDigest?.url.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/')
        : false;

    const date = txDigest?.timestampMs
        ? formatDate(txDigest.timestampMs, ['month', 'day', 'year'])
        : false;

    const transfersTxt = {
        nft: {
            header: 'Successfully Sent!',
        },
        coin: {
            header: 'SUI Transfer Completed!',
            copy: 'Staking SUI provides SUI holders with rewards to market price gains.',
        },
    };
    // TODO add copy for other trafer type like transfer sui, swap, etc.
    const headerCopy = tranferType
        ? transfersTxt[tranferType].header
        : `${txDigest.isSender ? 'Sent' : 'Received'} ${date || ''}`;
    const SuccessCard = (
        <>
            <div className={st.successIcon}>
                <Icon icon={TxIcon} className={iconClassName} />
            </div>
            <div className={st.successText}>{headerCopy}</div>
        </>
    );

    const failedCard = (
        <>
            <div className={st.failedIcon}>
                <div className={st.iconBg}>
                    <Icon icon={SuiIcons.Close} className={cl(st.close)} />
                </div>
            </div>
            <div className={st.failedText}>Failed</div>
            <div className={st.errorMessage}>{txDigest?.error}</div>
        </>
    );

    const fileExtentionType = useFileExtentionType(txDigest.url || '');

    const AssetCard = imgUrl && (
        <div className={st.wideview}>
            <div className={st.nftfields}>
                <div className={st.nftName}>{txDigest?.name}</div>
                <div className={st.nftType}>
                    {fileExtentionType?.name} {fileExtentionType?.type}
                </div>
            </div>
            <img
                className={cl(st.img)}
                src={imgUrl}
                alt={txDigest?.name || 'NFT'}
            />
        </div>
    );

    const statusClassName =
        txDigest.status === 'success' ? st.success : st.failed;

    return (
        <>
            <div className={st.txnResponse}>
                {txDigest.status === 'success' ? SuccessCard : failedCard}
                <div className={st.responseCard}>
                    {AssetCard}
                    {txDigest.amount && (
                        <div className={st.amount}>
                            {intl.formatNumber(
                                BigInt(txDigest.amount || 0),
                                balanceFormatOptions
                            )}{' '}
                            <span>{GAS_SYMBOL}</span>
                        </div>
                    )}
                    <div
                        className={cl(
                            st.txInfo,
                            !txDigest.isSender && st.reciever
                        )}
                    >
                        <div className={cl(st.txInfoLabel, statusClassName)}>
                            Your Wallet
                        </div>
                        <div className={cl(st.txInfoValue, statusClassName)}>
                            {txDigest.kind !== 'Call' && txDigest.isSender
                                ? txDigest.to
                                : txDigest.from}
                        </div>
                    </div>

                    {txDigest.txGas && (
                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>Gas Fee</div>
                            <div className={st.walletInfoValue}>
                                {txDigest.txGas} {GAS_SYMBOL}
                            </div>
                        </div>
                    )}

                    {txDigest.amount && (
                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>Total Amount</div>
                            <div className={st.walletInfoValue}>
                                {intl.formatNumber(
                                    BigInt(
                                        txDigest.amount + txDigest.txGas || 0
                                    ),
                                    balanceFormatOptions
                                )}{' '}
                                {GAS_SYMBOL}
                            </div>
                        </div>
                    )}

                    {date && (
                        <div className={st.txDate}>
                            <div className={st.txInfoLabel}>Date</div>
                            <div className={st.walletInfoValue}>{date}</div>
                        </div>
                    )}

                    {txDigest.txId && (
                        <div className={st.explorerLink}>
                            <ExplorerLink
                                type={ExplorerLinkType.transaction}
                                transactionID={txDigest.txId}
                                title="View on Sui Explorer"
                                className={st['explorer-link']}
                                showIcon={true}
                            >
                                View in Explorer
                            </ExplorerLink>
                        </div>
                    )}
                </div>
            </div>
        </>
    );
}

export default ReceiptCard;
