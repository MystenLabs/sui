import faker from '@faker-js/faker';

export function getTransaction(id?: string) {
    return {
        id: id || faker.datatype.hexaDecimal(32),
        sender: faker.datatype.hexaDecimal(32),
        status: faker.random.arrayElement(['success', 'fail']),
        created: Array.from({ length: faker.datatype.number(5) }, () =>
            faker.datatype.hexaDecimal(32)
        ),
        mutated: Array.from({ length: faker.datatype.number(5) }, () =>
            faker.datatype.hexaDecimal(32)
        ),
        deleted: Array.from({ length: faker.datatype.number(5) }, () =>
            faker.datatype.hexaDecimal(32)
        ),
    };
}
