
#[defines_primitive(vector)]
module std::vector {
    #[syntax(index)]
    native public fun vborrow<Element>(v: &vector<Element>, i: u64): &Element;
    #[syntax(index)]
    native public fun vborrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
    native public fun remove<Element>(v: &mut vector<Element>, i: u64): Element;
    native public fun length<Element>(v: &vector<Element>): u64;
}

module a::pool {
    public struct Order has store, drop { value: u8 }

    public fun find_match(
        orders: &mut vector<Order>,
    ): Option<Order> {
        let (mut i, len) = (0, orders.length());
        let mut matches = vector<u64>[];

        while (i < len) {
            i = i + 1;
        };

        let rnd = 100u256;
        let idx = rnd % (matches.length() as u256);
        let game = orders.remove(matches[idx]);
        option::some(game)
    }
}
