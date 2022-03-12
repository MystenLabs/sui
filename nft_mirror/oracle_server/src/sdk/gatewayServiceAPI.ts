import {
    paths as GatewayServicePaths,
    components,
} from './gateway-generated-schema';
import { Fetcher } from 'openapi-typescript-fetch';

export interface GatewayConnection {
    baseUrl: string;
}
export const gatewayServiceAPI = ({ baseUrl }: GatewayConnection) => {
    const fetcher = Fetcher.for<GatewayServicePaths>();

    fetcher.configure({
        baseUrl: baseUrl,
        init: {
            headers: {
                Accept: 'application/json',
            },
        },
    });

    return {
        getAddresses: fetcher.path('/addresses').method('get').create(),
        getObjects: fetcher.path('/objects').method('get').create(),
        getObjectInfo: fetcher.path('/object_info').method('get').create(),
        callMoveFunction: fetcher.path('/call').method('post').create(),
    };
};

export type CallRequest = components['schemas']['CallRequest'];
export type ObjectInfoResponse = components['schemas']['ObjectInfoResponse'];
