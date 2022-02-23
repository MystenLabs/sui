import { useParams } from 'react-router-dom';

const Search = () => {
    const { term } = useParams();
    return <h1>Search results for "{term}"</h1>;
};

export default Search;
