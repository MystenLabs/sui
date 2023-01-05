// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    is,
    NewObjectEvent,
    TransferObjectEvent,
    DeleteObjectEvent,
    PublishEvent,
    CoinBalanceChangeEvent,
    MutateObjectEvent,
    MoveEvent,
    EpochChangeEvent,
    CheckpointEvent,
    type SuiEvent,
    type SuiAddress,
    type ObjectId,
} from '@mysten/sui.js';

import { getOwnerStr } from '../../utils/objectUtils';
import { truncate } from '../../utils/stringUtils';

import type { Category } from '../../pages/transaction-result/TransactionResultType';
import type { LinkObj } from '../transaction-card/TxCardUtils';

export type ContentItem = {
    label: string;
    value: string;
    monotypeClass: boolean;
    link?: boolean;
    category?: Category;
};

export type EventDisplayData = {
    top: {
        title: string;
        content: (ContentItem | ContentItem[])[];
    };
    fields?: {
        title: string;
        // css class name to apply to the 'Fields' sub-header
        titleStyle?: string;
        content: (ContentItem | ContentItem[])[];
    };
};

function addressContent(label: string, addr: SuiAddress) {
    return {
        label: label,
        value: addr,
        link: true,
        category: 'address' as Category,
        monotypeClass: true,
    };
}

function objectContent(label: string, id: ObjectId) {
    return {
        label: label,
        value: id,
        link: true,
        category: 'object' as Category,
        monotypeClass: true,
    };
}

function fieldsContent(fields: { [key: string]: any } | undefined) {
    if (!fields) return [];
    return Object.keys(fields).map((k) => {
        return {
            label: k,
            value:
                typeof fields[k] === 'object'
                    ? JSON.stringify(fields[k])
                    : fields[k].toString(),
            monotypeClass: true,
        };
    });
}

function contentLine(
    label: string,
    value: string,
    monotypeClass: boolean = false
) {
    return {
        label,
        value,
        monotypeClass,
    };
}

export function moveEventDisplay(event: MoveEvent): EventDisplayData {
    return {
        top: {
            title: 'Move Event',
            content: [
                contentLine('Type', event.type, true),
                addressContent('Sender', event.sender as string),
                contentLine('BCS', event.bcs, true),
            ],
        },
        fields: {
            title: 'Fields',
            titleStyle: 'itemfieldstitle',
            content: fieldsContent(event.fields),
        },
    };
}

export function newObjectEventDisplay(event: NewObjectEvent): EventDisplayData {
    const packMod = `${event.packageId}::${event.transactionModule}`;

    return {
        top: {
            title: 'New Object',
            content: [
                contentLine('Module', packMod, true),
                [
                    addressContent('', event.sender),
                    addressContent('', getOwnerStr(event.recipient)),
                ],
            ],
        },
    };
}

export function transferObjectEventDisplay(
    event: TransferObjectEvent
): EventDisplayData {
    return {
        top: {
            title: 'Transfer Object',
            content: [
                contentLine('Object Type', event.objectType, true),
                objectContent('Object ID', event.objectId),
                contentLine('Version', event.version.toString()),
                [
                    addressContent('', event.sender),
                    addressContent('', getOwnerStr(event.recipient)),
                ],
            ],
        },
    };
}

export function mutateObjectEventDisplay(
    event: MutateObjectEvent
): EventDisplayData {
    return {
        top: {
            title: 'Mutate Object',
            content: [
                contentLine('Object Type', event.objectType, true),
                objectContent('Object ID', event.objectId),
                contentLine('Version', event.version.toString()),
                addressContent('', event.sender),
            ],
        },
    };
}

export function coinBalanceChangeEventDisplay(
    event: CoinBalanceChangeEvent
): EventDisplayData {
    return {
        top: {
            title: 'Coin Balance Change',
            content: [
                addressContent('Sender', event.sender),
                contentLine('Balance Change Type', event.changeType, true),
                contentLine('Coin Type', event.coinType),
                objectContent('Coin Object ID', event.coinObjectId),
                contentLine('Version', event.version.toString()),
                addressContent('Owner', getOwnerStr(event.owner)),
                contentLine('Amount', event.amount.toString()),
            ],
        },
    };
}

export function getAddressesLinks(item: ContentItem[]): LinkObj[] {
    return item
        .filter((itm) => !!itm.category)
        .map((content) => {
            return {
                url: content.value,
                name: truncate(content.value, 20),
                category: content.category,
            } as LinkObj;
        });
}

export function deleteObjectEventDisplay(
    event: DeleteObjectEvent
): EventDisplayData {
    const packMod = `${event.packageId}::${event.transactionModule}`;
    return {
        top: {
            title: 'Delete Object',
            content: [
                contentLine('Module', packMod, true),
                objectContent('Object ID', event.objectId),
                addressContent('Sender', event.sender),
            ],
        },
    };
}

export function publishEventDisplay(event: PublishEvent): EventDisplayData {
    return {
        top: {
            title: 'Publish',
            content: [
                addressContent('Sender', event.sender),
                contentLine('Package', event.packageId, true),
            ],
        },
    };
}

export function bigintDisplay(
    title: string,
    label: string,
    value: bigint | number
): EventDisplayData {
    return {
        top: {
            title: title,
            content: [contentLine(label, value.toString())],
        },
    };
}

export function eventToDisplay(event: SuiEvent) {
    if ('moveEvent' in event && is(event.moveEvent, MoveEvent))
        return moveEventDisplay(event.moveEvent);

    if ('newObject' in event && is(event.newObject, NewObjectEvent))
        return newObjectEventDisplay(event.newObject);

    if (
        'transferObject' in event &&
        is(event.transferObject, TransferObjectEvent)
    )
        return transferObjectEventDisplay(event.transferObject);

    if ('mutateObject' in event && is(event.mutateObject, MutateObjectEvent))
        return mutateObjectEventDisplay(event.mutateObject);

    if ('deleteObject' in event && is(event.deleteObject, DeleteObjectEvent))
        return deleteObjectEventDisplay(event.deleteObject);

    if (
        'coinBalanceChange' in event &&
        is(event.coinBalanceChange, CoinBalanceChangeEvent)
    )
        return coinBalanceChangeEventDisplay(event.coinBalanceChange);

    if ('publish' in event && is(event.publish, PublishEvent))
        return publishEventDisplay(event.publish);

    // TODO - once epoch and checkpoint pages exist, make these links
    if ('epochChange' in event && is(event.epochChange, EpochChangeEvent))
        return bigintDisplay('Epoch Change', 'Epoch ID', event.epochChange);

    if ('checkpoint' in event && is(event.checkpoint, CheckpointEvent))
        return bigintDisplay('Checkpoint', 'Sequence #', event.checkpoint);

    return null;
}
