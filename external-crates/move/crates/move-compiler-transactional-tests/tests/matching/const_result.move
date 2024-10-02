//# init --edition 2024.beta

//# publish
module 0x42::m {

    const EInvalidName: u64 = 10;
    const EInvalidInfo: u64 = 20;
    const EInvalidCoin: u64 = 30;

    public fun invalid_name_error(): u64 {
        EInvalidName
    }

    public fun invalid_info_error(): u64 {
        EInvalidInfo
    }

    public fun invalid_coin_error(): u64 {
        EInvalidCoin
    }

    public struct QueryResult<T> { code: u64, value: T }

    public fun create_query_result<T>(code: u64, value: T): QueryResult<T> {
        QueryResult { code, value }
    }

    public fun fix_name<T>(q: QueryResult<T>): QueryResult<T> {
        let QueryResult { value, code: _ } = q;
        QueryResult { value: value, code: 0 }
    }

    public fun fix_info<T>(q: QueryResult<T>): QueryResult<T> {
        let QueryResult { value, code: _ } = q;
        QueryResult { value: value, code: 0 }
    }

    public fun fix_coin<T>(q: QueryResult<T>): QueryResult<T> {
        let QueryResult { value, code: _ } = q;
        QueryResult { value: value, code: 0 }
    }

    public fun test<T>(query: QueryResult<T>): QueryResult<T> {
        let query = match (&query) {
            QueryResult { code: EInvalidName, .. } => query.fix_name(),
            QueryResult { code: EInvalidInfo, .. } => query.fix_info(),
            QueryResult { code: EInvalidCoin, .. } => query.fix_coin(),
            QueryResult { code: code, .. } if (*code > 0) => query,
            _ => query,
        };
        query
    }

    public fun valid<T>(query: &QueryResult<T>): bool {
        query.code == 0
    }

    public fun destroy_query<T>(q: QueryResult<T>): T {
        let QueryResult { value, .. } = q;
        value
    }

}

//# run
module 0x43::main {
    use 0x42::m;

    fun main() {
        let name_query = m::create_query_result(m::invalid_name_error(), 0);
        assert!(!m::valid(&name_query), 0);
        let name_query = m::test(name_query);
        assert!(m::valid(&name_query), 1);
        let _ = m::destroy_query(name_query);

        let info_query = m::create_query_result(m::invalid_info_error(), 0);
        assert!(!m::valid(&info_query), 2);
        let info_query = m::test(info_query);
        assert!(m::valid(&info_query), 3);
        let _ = m::destroy_query(info_query);

        let coin_query = m::create_query_result(m::invalid_coin_error(), 0);
        assert!(!m::valid(&coin_query), 4);
        let coin_query = m::test(coin_query);
        assert!(m::valid(&coin_query), 5);
        let _ = m::destroy_query(coin_query);

        let some_query = m::create_query_result(40, 0);
        assert!(!m::valid(&some_query), 6);
        let some_query = m::test(some_query);
        assert!(!m::valid(&some_query), 7);
        let _ = m::destroy_query(some_query);

        let okay_query = m::create_query_result(0, 0);
        assert!(m::valid(&okay_query), 8);
        let _ = m::destroy_query(okay_query);
    }

}
