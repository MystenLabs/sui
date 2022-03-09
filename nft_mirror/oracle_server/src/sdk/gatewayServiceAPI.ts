import { paths as GatewayServicePaths } from './gateway-generated-schema';
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
    };
};
