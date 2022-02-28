import faker from '@faker-js/faker';

import { getObject } from './objects';
import { getTransaction } from './transactions';

export function getSearchResponse() {
    const totalItems = faker.datatype.number(5);
    return Array.from({ length: totalItems }, () => {
        const type = faker.random.arrayElement(['tx', 'obj']);
        const item = type === 'tx' ? getTransaction() : getObject();
        return {
            type,
            item,
        };
    });
}
