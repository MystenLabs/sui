import mockTransactionData from './mock_data.json';

const navigateWithUnknown = (input: string, navigate: Function) => {
    const data = findDataFromID(input, false);
    if (data === undefined || !('category' in data)) {
        navigate(`../missing/${input}`);
    } else if (data.category === 'transaction') {
        navigate(`../transactions/${input}`, { state: data });
    } else if (data.category === 'object') {
        navigate(`../objects/${input}`, { state: data });
    } else if (data.category === 'address') {
        navigate(`../addresses/${input}`, { state: data });
    } else {
        navigate(`../missing/${input}`);
    }
};

const findDataFromID = (targetID: string | undefined, state: any) =>
    state?.category !== undefined
        ? state
        : mockTransactionData.data.find(({ id }) => id === targetID);

export { findDataFromID, navigateWithUnknown };
