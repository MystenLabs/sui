#[defines_primitive(vector)]
module std::vector {

    #[bytecode_instruction]
    /// Return the length of the vector.
    native public fun length<Element>(v: &vector<Element>): u64;

    #[syntax(index)]
    #[bytecode_instruction]
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[syntax(index)]
    #[bytecode_instruction]
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

    #[bytecode_instruction]
    native public fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64);

    #[bytecode_instruction]
    native public fun pop_back<Element>(v: &mut vector<Element>): Element;

    #[bytecode_instruction]
    native public fun destroy_empty<Element>(v: vector<Element>);

    public fun reverse<Element>(v: &mut vector<Element>) {
        let len = v.length();
        if (len == 0) return ();

        let mut front_index = 0;
        let mut back_index = len -1;
        while (front_index < back_index) {
            v.swap(front_index, back_index);
            front_index = front_index + 1;
            back_index = back_index - 1;
        }
    }

    public fun is_empty<Element>(v: &vector<Element>): bool {
        v.length() == 0
    }

    #[syntax(for)]
    public macro fun for_each<$T>($v: vector<$T>, $body: |$T|) {
        let mut v = $v;
        reverse(&mut v);
        while (!v.is_empty()) {
            let next = v.pop_back();
            $body(next);
        };
        v.destroy_empty();
    }

    #[syntax(for)]
    public macro fun for_imm<$T>($v: &vector<$T>, $body: |&$T|) {
        let v = $v;
        let mut i = 0;
        let len = v.length();
        while (i < len) {
            $body(&v[i]);
            i = i + 1;
        }
    }

    #[syntax(for)]
    public macro fun for_mut<$T>($v: &mut vector<$T>, $body: |&mut $T|) {
        let v = $v;
        let mut i = 0;
        let len = v.length();
        while (i < len) {
            $body(&mut v[i]);
            i = i + 1;
        }
    }
}

// Correct usage
module a::valid {

    public fun test(): u64 {

        let mut sum = 0;
        let v1 = vector[1,2,3];
        let v2 = vector[1,2,3];
        let mut v3 = vector[1,2,3];

        for (x as v1) {
            sum = sum + x;
        };

        for (x as &v2) {
            sum = sum + *x;
        };

        for (x as &mut v3) {
            sum = sum + *x;
        };

        sum
    }

}
