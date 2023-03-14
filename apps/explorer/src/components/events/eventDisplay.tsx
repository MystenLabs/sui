// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export {};

// import {
//     is,
//     NewObjectEvent,
//     TransferObjectEvent,
//     DeleteObjectEvent,
//     PublishEvent,
//     CoinBalanceChangeEvent,
//     MutateObjectEvent,
//     MoveEvent,
//     EpochChangeEvent,
//     CheckpointEvent,
//     type SuiEvent,
//     type SuiAddress,
//     type ObjectId,
//     formatAddress,
//     isEventType,
//     getMoveEvent,
//     getNewObjectEvent,
//     getTransferObjectEvent,
//     getMutateObjectEvent,
//     getDeletObjectEvent,
//     getCoinBalanceChangeEvent,
//     getPublishEvent,
//     getEpochChangeEvent,
//     getCheckpointEvent,
// } from '@mysten/sui.js';

// import { getOwnerStr } from '../../utils/objectUtils';

// import type { Category } from '../../pages/transaction-result/TransactionResultType';
// import type { LinkObj } from '../transaction-card/TxCardUtils';

// export type ContentItem = {
//     label: string;
//     value: string;
//     monotypeClass: boolean;
//     link?: boolean;
//     category?: Category;
// };

// export type EventDisplayData = {
//     top: {
//         title: string;
//         content: (ContentItem | ContentItem[])[];
//     };
//     fields?: {
//         title: string;
//         // css class name to apply to the 'Fields' sub-header
//         titleStyle?: string;
//         content: (ContentItem | ContentItem[])[];
//     };
// };

// function addressContent(label: string, addr: SuiAddress) {
//     return {
//         label: label,
//         value: addr,
//         link: true,
//         category: 'address' as Category,
//         monotypeClass: true,
//     };
// }

// function objectContent(label: string, id: ObjectId) {
//     return {
//         label: label,
//         value: id,
//         link: true,
//         category: 'object' as Category,
//         monotypeClass: true,
//     };
// }

// function fieldsContent(fields: { [key: string]: any } | undefined) {
//     if (!fields) return [];
//     return Object.keys(fields).map((k) => ({
//         label: k,
//         value:
//             typeof fields[k] === 'object'
//                 ? JSON.stringify(fields[k])
//                 : fields[k].toString(),
//         monotypeClass: true,
//     }));
// }

// function contentLine(
//     label: string,
//     value: string,
//     monotypeClass: boolean = false
// ) {
//     return {
//         label,
//         value,
//         monotypeClass,
//     };
// }

// export function moveEventDisplay(event: MoveEvent): EventDisplayData {
//     const packMod = `${event.packageId}::${event.transactionModule}`;
//     return {
//         top: {
//             title: 'Move Event',
//             content: [
//                 contentLine('Module', packMod, true),
//                 contentLine('Type', event.type, true),
//                 addressContent('Sender', event.sender as string),
//                 contentLine('BCS', event.bcs, true),
//             ],
//         },
//         fields: {
//             title: 'Fields',
//             titleStyle: 'itemfieldstitle',
//             content: fieldsContent(event.fields),
//         },
//     };
// }

// export function newObjectEventDisplay(event: NewObjectEvent): EventDisplayData {
//     const packMod = `${event.packageId}::${event.transactionModule}`;

//     return {
//         top: {
//             title: 'New Object',
//             content: [
//                 contentLine('Module', packMod, true),
//                 contentLine('Object Type', event.objectType),
//                 objectContent('Object ID', event.objectId),
//                 contentLine('Version', event.version.toString()),
//                 [
//                     addressContent('', event.sender),
//                     addressContent('', getOwnerStr(event.recipient)),
//                 ],
//             ],
//         },
//     };
// }

// export function transferObjectEventDisplay(
//     event: TransferObjectEvent
// ): EventDisplayData {
//     const packMod = `${event.packageId}::${event.transactionModule}`;
//     return {
//         top: {
//             title: 'Transfer Object',
//             content: [
//                 contentLine('Module', packMod, true),
//                 contentLine('Object Type', event.objectType, true),
//                 objectContent('Object ID', event.objectId),
//                 contentLine('Version', event.version.toString()),
//                 [
//                     addressContent('', event.sender),
//                     addressContent('', getOwnerStr(event.recipient)),
//                 ],
//             ],
//         },
//     };
// }

// export function mutateObjectEventDisplay(
//     event: MutateObjectEvent
// ): EventDisplayData {
//     const packMod = `${event.packageId}::${event.transactionModule}`;
//     return {
//         top: {
//             title: 'Mutate Object',
//             content: [
//                 contentLine('Module', packMod, true),
//                 contentLine('Object Type', event.objectType, true),
//                 objectContent('Object ID', event.objectId),
//                 contentLine('Version', event.version.toString()),
//                 addressContent('', event.sender),
//             ],
//         },
//     };
// }

// export function coinBalanceChangeEventDisplay(
//     event: CoinBalanceChangeEvent
// ): EventDisplayData {
//     return {
//         top: {
//             title: 'Coin Balance Change',
//             content: [
//                 addressContent('Sender', event.sender),
//                 contentLine('Balance Change Type', event.changeType, true),
//                 contentLine('Coin Type', event.coinType),
//                 objectContent('Coin Object ID', event.coinObjectId),
//                 contentLine('Version', event.version.toString()),
//                 addressContent('Owner', getOwnerStr(event.owner)),
//                 contentLine('Amount', event.amount.toString()),
//             ],
//         },
//     };
// }

// export function getAddressesLinks(item: ContentItem[]): LinkObj[] {
//     return item
//         .filter((itm) => !!itm.category)
//         .map(
//             (content) =>
//                 ({
//                     url: content.value,
//                     name: formatAddress(content.value),
//                     category: content.category,
//                 } as LinkObj)
//         );
// }

// export function deleteObjectEventDisplay(
//     event: DeleteObjectEvent
// ): EventDisplayData {
//     const packMod = `${event.packageId}::${event.transactionModule}`;
//     return {
//         top: {
//             title: 'Delete Object',
//             content: [
//                 contentLine('Module', packMod, true),
//                 objectContent('Object ID', event.objectId),
//                 addressContent('Sender', event.sender),
//             ],
//         },
//     };
// }

// export function publishEventDisplay(event: PublishEvent): EventDisplayData {
//     return {
//         top: {
//             title: 'Publish',
//             content: [
//                 addressContent('Sender', event.sender),
//                 contentLine('Package', event.packageId, true),
//             ],
//         },
//     };
// }

// export function bigintDisplay(
//     title: string,
//     label: string,
//     value: bigint | number
// ): EventDisplayData {
//     return {
//         top: {
//             title: title,
//             content: [contentLine(label, value.toString())],
//         },
//     };
// }

// export function eventToDisplay(event: SuiEvent) {
//     if (isEventType(event, 'moveEvent') && is(event.content, MoveEvent))
//         return moveEventDisplay(getMoveEvent(event)!);

//     if (isEventType(event, 'newObject') && is(event.content, NewObjectEvent))
//         return newObjectEventDisplay(getNewObjectEvent(event)!);

//     if (
//         isEventType(event, 'transferObject') &&
//         is(event.content, TransferObjectEvent)
//     )
//         return transferObjectEventDisplay(getTransferObjectEvent(event)!);

//     if (
//         isEventType(event, 'mutateObject') &&
//         is(event.content, MutateObjectEvent)
//     )
//         return mutateObjectEventDisplay(getMutateObjectEvent(event)!);

//     if (
//         isEventType(event, 'deleteObject') &&
//         is(event.content, DeleteObjectEvent)
//     )
//         return deleteObjectEventDisplay(getDeletObjectEvent(event)!);

//     if (
//         isEventType(event, 'coinBalanceChange') &&
//         is(event.content, CoinBalanceChangeEvent)
//     )
//         return coinBalanceChangeEventDisplay(getCoinBalanceChangeEvent(event)!);

//     if (isEventType(event, 'publish') && is(event.content, PublishEvent))
//         return publishEventDisplay(getPublishEvent(event)!);

//     // TODO - once epoch and checkpoint pages exist, make these links
//     if (
//         isEventType(event, 'epochChange') &&
//         is(event.content, EpochChangeEvent)
//     )
//         return bigintDisplay(
//             'Epoch Change',
//             'Epoch ID',
//             getEpochChangeEvent(event)!
//         );

//     if (isEventType(event, 'checkpoint') && is(event.content, CheckpointEvent))
//         return bigintDisplay(
//             'Checkpoint',
//             'Sequence #',
//             getCheckpointEvent(event)!
//         );

//     return null;
// }
