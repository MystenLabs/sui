// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export {};

// import {
//     getCoinBalanceChangeEvent,
//     getTransferObjectEvent,
//     isEventType,
//     type TransactionEvents,
// } from '@mysten/sui';

// export type CoinsMetaProps = {
//     amount: number;
//     coinType: string;
//     receiverAddress: string;
// };

// export type TxnMetaResponse = {
//     objectIDs: string[];
//     coins: CoinsMetaProps[];
// };

// export function getEventsSummary(
//     events: TransactionEvents,
//     address: string
// ): TxnMetaResponse {
//     const coinsMeta = {} as { [coinType: string]: CoinsMetaProps };
//     const objectIDs: string[] = [];

//     events.forEach((event) => {
//         // Aggregate coinBalanceChange by coinType and address
//         // A net positive amount means the user received coins
//         // A net negative amount means the user sent coins
//         if (
//             event.type === 'coinBalanceChange' &&
//             event?.content?.changeType &&
//             ['Receive', 'Pay'].includes(event?.content?.changeType)
//         ) {
//             const coinBalanceChange = getCoinBalanceChangeEvent(event)!;
//             const { coinType, amount, owner, sender } = coinBalanceChange;

//             const AddressOwner =
//                 owner !== 'Immutable' && 'AddressOwner' in owner
//                     ? owner.AddressOwner
//                     : null;

//             // ChangeEpoch txn includes coinBalanceChange event for other addresses
//             if (
//                 AddressOwner === address ||
//                 (address === sender && AddressOwner)
//             ) {
//                 coinsMeta[`${AddressOwner}${coinType}`] = {
//                     amount:
//                         (coinsMeta[`${AddressOwner}${coinType}`]?.amount || 0) +
//                         +amount,
//                     coinType: coinType,
//                     receiverAddress: AddressOwner,
//                 };
//             }
//         }

//         // return objectIDs of the transfer objects
//         if (isEventType(event, 'transferObject')) {
//             const transferObject = getTransferObjectEvent(event)!;
//             const { AddressOwner } = transferObject.recipient as {
//                 AddressOwner: string;
//             };
//             if (AddressOwner === address) {
//                 objectIDs.push(transferObject?.objectId);
//             }
//         }
//     });

//     return {
//         objectIDs,
//         coins: Object.values(coinsMeta),
//     };
// }
