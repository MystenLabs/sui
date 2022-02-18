import { Link } from 'react-router-dom';

const Home = () => {
    return (
        <>
            <h1>This is home page</h1>
            <ul>
                <li>
                    See details for transaction{' '}
                    <Link to="/transactions/tx1">#tx1</Link>
                </li>
                <li>
                    See search results for <Link to="/search/aTerm">aTerm</Link>
                </li>
            </ul>
        </>
    );
};

export default Home;
