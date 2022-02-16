import { Link } from 'react-router-dom';

const NavigationHeader = () => {
    return (
        <header>
            <nav>
                <Link to="/">Sui Explorer</Link>
            </nav>
        </header>
    );
};

export default NavigationHeader;
