module 0x42::m {
    public enum Temperature {
       Fahrenheit(u16),
       Celsius { temp: u16 },
       Unknown
    }

    fun is_temperature_fahrenheit(t: &Temperature): bool {
       match (t) {
          Temperature::Fahrenheit(_) => true,
          _ => false,
       }
    }

    fun is_temperature_boiling(t: &Temperature): bool {
       match (t) {
          Temperature::Fahrenheit(temp) if (*temp >= 212) => true,
          Temperature::Celsius { temp } if (*temp >= 100) => true,
          _ => false,
       }
    }

    public enum Option<T> {
      Some(T),
      None
    }

    public fun is_some_true_0(o: Option<bool>): bool {
       match (o) {
         Option::Some(true) => true,
         Option::Some(_) => false,
         Option::None => false,
       }
    }

    public fun is_some_true_1(o: Option<bool>): bool {
       match (o) {
         Option::Some(true) => true,
         Option::Some(false) => false,
         Option::None => false,
       }
    }

    public fun is_some_true_2(o: Option<bool>): bool {
       match (o) {
         Option::Some(x) => x,
         Option::None => false,
       }
    }

    public fun option_default<T: drop>(o: Option<T>, default: T): Option<T> {
       match (o) {
         x @ _ => x,
         Option::None => Option::Some(default),
       }
    }

    public enum Expression has drop {
       Done,
       Add,
       Mul,
       Num(u64),
    }

    public fun evaluate(expressions: &mut vector<Expression>): u64 {
        use 0x42::m::Expression as E;
        let mut stack = vector[];
        while (!expressions.is_empty()) {
            match (expressions.pop_back()) {
                E::Done => break,
                E::Add => {
                    let e1 = stack.pop_back();
                    let e2 = stack.pop_back();
                    stack.push_back(e1 + e2);
                },
                E::Mul => {
                    let e1 = stack.pop_back();
                    let e2 = stack.pop_back();
                    stack.push_back(e1 * e2);
                },
                E::Num(number) => {
                    stack.push_back(number);
                }
            }
        };
        let result = stack.pop_back();
        assert!(expressions.is_empty(), 0);
        assert!(stack.is_empty(), 1);
        result
    }

    public fun count_numbers(expressions: &mut vector<Expression>): u64 {
        use 0x42::m::Expression as E;
        let mut n = 0;
        while (!expressions.is_empty()) {
            match (expressions.pop_back()) {
                E::Add | E::Mul => (),
                E::Num(_) => {
                    n = n + 1;
                },
                E::Done => return n,
            }
        };
        n
    }

    public fun count_ops(expressions: &mut vector<Expression>): u64 {
        use 0x42::m::Expression as E;
        let mut n = 0;
        while (!expressions.is_empty()) {
            match (expressions.pop_back()) {
                E::Add | E::Mul => {
                    n = n + 1;
                },
                _ => (),
            }
        };
        n
    }

    public fun has_done(expressions: &mut vector<Expression>): bool {
        use 0x42::m::Expression as E;
        while (!expressions.is_empty()) {
            match (expressions.pop_back()) {
                E::Add | E::Mul | E::Num(_) => (),
                E::Done => { return true },
            }
        };
        false
    }

}

#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    native public fun empty<Element>(): vector<Element>;

    #[bytecode_instruction]
    native public fun length<Element>(v: &vector<Element>): u64;

    public fun is_empty<Element>(v: &vector<Element>): bool {
        v.length() == 0
    }

    #[bytecode_instruction]
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[bytecode_instruction]
    native public fun push_back<Element>(v: &mut vector<Element>, e: Element);

    #[bytecode_instruction]
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

    #[bytecode_instruction]
    native public fun pop_back<Element>(v: &mut vector<Element>): Element;

    #[bytecode_instruction]
    native public fun destroy_empty<Element>(v: vector<Element>);

    #[bytecode_instruction]
    native public fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64);
}
