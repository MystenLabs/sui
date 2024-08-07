

module move_2024::public_types {


public struct Users has key, store {
    id: UID,
    users: vector<address>
}


}

module move_2024::positional_structs {

public struct Point(u64, u64) has copy, drop;
public struct Potato()

public fun new_point(x: u64, y: u64): Point {
    Point(x, y)
}

public fun move_to(p: &mut Point, x: u64, y: u64) {
    p.0 = x;
    p.1 = y;
}
}

module move_2024::method_aliases {

public struct Users {

}

public fun users(list: &Users): vector<address> { &list.users }

fun test_list() {
    // ...
    let user_count = list.users().length();
}
}

module move_2024::implicit_imports {
    public struct Users has key, store {
        id: UID,
        users: vector<address>
    }

    public fun new(ctx: &mut TxContext) {
        let users = Users {
            id: object::new(ctx),
            users: vector[]
        };

        transfer::transfer(users, ctx.sender());
    }
}
