module A::mod0;

public fun test_bytestring(): vector<u8> { "hello world" }
public fun test_ascii(): std::ascii::String { "hello world" }
public fun test_utf8(): std::string::String { "hello world" }
