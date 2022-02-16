import { useParams } from 'react-router-dom';

const TransactionDetails = () => {
    const { id: txID } = useParams();
    return <h1>Transaction #{txID}</h1>;
};

export default TransactionDetails;
