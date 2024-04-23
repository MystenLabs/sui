//# init --edition 2024.alpha


//# publish
module 0x42::my_vec {

    public struct MyVec<T> has drop { v: vector<T> }

    public fun make_vec<T>(v: vector<T>): MyVec<T> {
        MyVec { v }
    }

    public fun reverse<Element>(v: &mut MyVec<Element>) {
        let len = v.v.length();
        if (len == 0) return ();

        let mut front_index = 0;
        let mut back_index = len -1;
        while (front_index < back_index) {
            v.v.swap(front_index, back_index);
            front_index = front_index + 1;
            back_index = back_index - 1;
        }
    }

    public fun get_vec<Element>(v: MyVec<Element>): vector<Element> {
        let MyVec { v } = v;
        v
    }

    public fun borrow_vec<Element>(v: &MyVec<Element>): &vector<Element> {
        &v.v
    }

    public fun borrow_mut_vec<Element>(v: &mut MyVec<Element>): &mut vector<Element> {
        &mut v.v
    }

    public fun is_empty<Element>(v: &MyVec<Element>): bool {
        v.v.length() == 0
    }

    #[syntax(for)]
    public macro fun for_each<$T>($v: MyVec<$T>, $body: |$T|) {
        let mut v = get_vec($v);
        v.reverse();
        while (!v.is_empty()) {
            let next = v.pop_back();
            $body(next);
        };
        v.destroy_empty();
    }

    #[syntax(for)]
    public macro fun for_imm<$T>($v: &MyVec<$T>, $body: |&$T|) {
        let v = borrow_vec($v);
        let mut i = 0;
        let len = v.length();
        while (i < len) {
            $body(v.borrow(i));
            i = i + 1;
        }
    }

    #[syntax(for)]
    public macro fun for_mut<$T>($v: &mut MyVec<$T>, $body: |&mut $T|) {
        let v = borrow_mut_vec($v);
        let mut i = 0;
        let len = v.length();
        while (i < len) {
            $body(v.borrow_mut(i));
            i = i + 1;
        }
    }
}

//# run
module 0x42::valid {

    use 0x42::my_vec::make_vec;

    public fun test() {

        let mut sum = 0;
        let v1 = make_vec(vector[1,2,3]);
        let v2 = make_vec(vector[1,2,3]);
        let mut v3 = make_vec(vector[1,2,3]);

        for (x in v1) {
            sum = sum + x;
        };

        for (x in &v2) {
            sum = sum + *x;
        };

        for (x in &mut v3) {
            sum = sum + *x;
        };

        assert!(sum == 18, 0);
    }

}
