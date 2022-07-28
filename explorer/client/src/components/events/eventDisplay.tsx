import { getOwnerStr } from '../../utils/objectUtils';

import type { MoveEvent, NewObjectEvent, TransferObjectEvent } from '@mysten/sui.js';

export function moveEventDisplay(event: MoveEvent) {
    return {
        top: {
            title: 'Move Event',
            content: [
                {
                    label: 'Type',
                    value: event.type,
                    monotypeClass: true,
                },
                {
                    label: 'Sender',
                    value: event.sender,
                    monotypeClass: true,
                },
                {
                    label: 'BCS',
                    value: event.bcs,
                    monotypeClass: true,
                },
            ],
        },
        fields: {
            title: 'Fields',
            titleStyle: 'itemfieldstitle',
            content: fieldsContent(event.fields),
        },
    };
}

export function newObjectEventDisplay(event: NewObjectEvent) {
    return {
        top: {
            title: 'New Object',
            content: [
                {
                    label: 'Module',
                    value: `${event.packageId}::${event.transactionModule}`,
                    monotypeClass: true,
                },
                {
                    label: 'Sender',
                    value: event.sender,
                    monotypeClass: true,
                },
                {
                    label: 'Recipient',
                    value: getOwnerStr(event.recipient),
                    monotypeClass: true,
                },
            ],
        },
        fields: null,
    };
}

export function transferObjectEventDisplay(event: TransferObjectEvent) {
    return {
        top: {
            title: 'Transfer Object',
            content: [
                {
                    label: 'Type',
                    value: event.type,
                    monotypeClass: true,
                },
                {
                    label: 'Object ID',
                    value: event.objectId,
                    monotypeClass: true,
                },
                {
                    label: 'Version',
                    value: event.version.toString(),
                    monotypeClass: false,
                },
                {
                    label: 'Sender',
                    value: event.sender,
                    monotypeClass: true,
                },
                {
                    label: 'Recipient',
                    value: getOwnerStr(event.recipient),
                    monotypeClass: true,
                },
            ],
        },
        fields: null
    };
}

function fieldsContent(fields: { [key: string]: any }) {
    return Object.keys(fields).map((k) => {
        return {
            label: k,
            value: fields[k].toString(),
            monotypeClass: true,
        };
    });
}
