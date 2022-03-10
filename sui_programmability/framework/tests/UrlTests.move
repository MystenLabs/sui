#[test_only]
module Sui::UrlTests {
    use Sui::Url;
    use Sui::TxContext;
    use Std::ASCII::Self;

    const HASH_VECTOR_LENGTH: u64 = 32;
    const HASH_LENGTH_MISMATCH: u64 = 0;
    const URL_STRING_MISMATCH: u64 = 1;

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_malformed_hash() {
        let ctx = TxContext::dummy();
        // url strings are not currently validated
        let url_str = ASCII::string(x"414243454647");
        // length too short
        let hash = x"badf012345";

        let url = Url::new(url_str, hash, &mut ctx);

        assert!(Url::get_resource_hash(&url) == hash, HASH_LENGTH_MISMATCH);
        assert!(Url::get_url(&url) == url_str, URL_STRING_MISMATCH);

        Url::delete(url);
    }

    #[test]
    fun test_good_hash() {
        let ctx = TxContext::dummy();
        // url strings are not currently validated
        let url_str = ASCII::string(x"414243454647");
        // 32 bytes
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef";

        let url = Url::new(url_str, hash, &mut ctx);

        assert!(Url::get_resource_hash(&url) == hash, HASH_LENGTH_MISMATCH);
        assert!(Url::get_url(&url) == url_str, URL_STRING_MISMATCH);

        let url_str = ASCII::string(x"37414243454647");

        Url::update(&mut url, url_str);
        assert!(Url::get_url(&url) == url_str, URL_STRING_MISMATCH);

        Url::delete(url);
    }
}