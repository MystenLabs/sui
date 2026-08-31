// Unexpanded macro bodies with private constants are no concern for visibility

module 0x42::a {

const SECRET: u64 = 42;

public macro fun get_secret(): u64 {
    SECRET
}

}
