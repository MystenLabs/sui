import faker from '@faker-js/faker';

export function getObject() {
    return {
        id: faker.datatype.hexaDecimal(32),
        type: faker.datatype.uuid(),
    };
}
