module 0x42::m {

    const EInvalidName: u64 = 10;
    const EInvalidInfo: u64 = 20;
    const EInvalidCoin: u64 = 10;

    public enum Option<T> {
        Some(T),
        None
    }

    public struct QueryResult<T> { code: Option<u64>, value: T }

    fun create_query_result<T>(): QueryResult<T> { abort 0 }

    fun fix_name<T>(_q: QueryResult<T>): QueryResult<T> { abort 0 }
    fun fix_info<T>(_q: QueryResult<T>): QueryResult<T> { abort 0 }
    fun fix_coin<T>(_q: QueryResult<T>): QueryResult<T> { abort 0 }

    fun test<T>(): QueryResult<T> {
        let query = create_query_result();
        let query = match (&query) {
            QueryResult { code: Option::Some(EInvalidName), .. } => fix_name(query),
            QueryResult { code: Option::Some(EInvalidInfo), .. } => fix_info(query),
            QueryResult { code: Option::Some(EInvalidCoin), .. } => fix_coin(query),
            QueryResult { code: Option::Some(code), .. } if (*code > 0) => abort 0,
            _ => query,
        };
        query
    }

}
