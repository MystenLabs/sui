import { getOwnerStr } from '../../utils/objectUtils';

import type {
    MoveEvent,
    NewObjectEvent,
    ObjectId,
    SuiAddress,
    TransferObjectEvent,
} from '@mysten/sui.js';

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
                addressContent('Sender', event.sender as string),
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

function addressContent(label: string, addr: SuiAddress) {
    return {
        label: label,
        value: addr,
        link: true,
        category: 'addresses',
        monotypeClass: true,
    }
}

function objectContent(label: string, id: ObjectId) {
    return {
        label: label,
        value: id,
        link: true,
        category: 'objects',
        monotypeClass: true,
    }
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
                addressContent('Sender', event.sender),
                addressContent('Recipient', getOwnerStr(event.recipient)),
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
                objectContent('Object ID', event.objectId),
                {
                    label: 'Version',
                    value: event.version.toString(),
                    monotypeClass: false,
                },
                addressContent('Sender', event.sender),
                addressContent('Recipient', getOwnerStr(event.recipient)),
            ],
        },
        fields: null,
    };
}
