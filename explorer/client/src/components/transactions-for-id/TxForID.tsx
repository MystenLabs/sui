import { useState, useEffect, useContext } from 'react';

import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import ErrorResult from '../error-result/ErrorResult';
import Longtext from '../longtext/Longtext';

const DATATYPE_DEFAULT = {
    to: [],
    from: [],
    loadState: 'pending',
};

const getTx = async (id: string, network: string, category: 'address') =>
    rpc(network).getTransactionsForAddress(id);

export default function TxForID({
    id,
    category,
}: {
    id: string;
    category: 'address';
}) {
    const [showData, setData] = useState(DATATYPE_DEFAULT);
    const [network] = useContext(NetworkContext);
    const deduplicate = (results: [string, string][]) =>
        results
            .map((result) => result[1])
            .filter((value, index, self) => self.indexOf(value) === index);

    useEffect(() => {
        getTx(id, network, category)
            .then((data) =>
                setData({
                    ...(data as typeof DATATYPE_DEFAULT),
                    loadState: 'loaded',
                })
            )
            .catch((error) => {
                console.log(error);
                setData({ ...DATATYPE_DEFAULT, loadState: 'fail' });
            });
    }, [id, network, category]);

    if (showData.loadState === 'pending') {
        return <div>Loading ...</div>;
    }

    if (showData.loadState === 'loaded') {
        return (
            <>
                <div>
                    <div>Transactions Sent</div>
                    <div>
                        {deduplicate(showData.from).map((x, index) => (
                            <div key={`from-${index}`}>
                                <Longtext
                                    text={x}
                                    category="transactions"
                                    isLink={true}
                                />
                            </div>
                        ))}
                    </div>
                </div>
                <div>
                    <div>Transactions Received</div>
                    <div>
                        {deduplicate(showData.to).map((x, index) => (
                            <div key={`to-${index}`}>
                                <Longtext
                                    text={x}
                                    category="transactions"
                                    isLink={true}
                                />
                            </div>
                        ))}
                    </div>
                </div>
            </>
        );
    }
    return (
        <ErrorResult
            id={id}
            errorMsg="Transactions could not be extracted on the following specified transaction ID"
        />
    );
}
