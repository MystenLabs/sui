module A::mod5 {

    struct UID { }
    struct TxContext { }

    fun empty<T>(): vector<T> { abort 0}
    fun push_back<T>(_v: &mut vector<T>, _t: T) { abort 0 }
    fun pop_back<T>(_v: &mut vector<T>): T { abort 0 }

    fun new(_ctxt: &mut TxContext): UID { abort 0 }
    fun delete(_id: UID) { abort 0 }

    public entry fun delete_n_ids(n: u64, ctx: &mut TxContext) {
        let v: vector<UID> = empty();
        let i = 0;
        while (i < n) {
            let id = new(ctx);
            push_back(&mut v, id);
            i = i + 1;
        };
        i = 0;
        while (i < n) {
            let id = pop_back(&mut v);
            delete(id);
            i = i + 1;
        };
        vector::destroy_empty(v);
    }
}
