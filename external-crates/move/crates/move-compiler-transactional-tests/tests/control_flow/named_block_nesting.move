//# init --edition 2024.alpha

//# publish

#[allow(dead_code,unused_assignment)]
module 42::m {

    entry fun t00() {
        loop 'a: {
            loop { break 'a }
        }
    }

    entry fun t01() {
        loop 'a: {
            'b: { break 'a }
        }
    }

    entry fun t02() {
        loop 'a: {
            (loop { break 'a } : ())
        }
    }

    entry fun t03() {
        loop 'a: {
            ('b: { break 'a } : ())
        }
    }

    entry fun t04() {
        let x = loop 'a: {
            (loop { break 'a 0 } : ())
        };
        assert!(x == 0, 42);
    }

    entry fun t05() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            (loop { continue 'a } : ())
        };
    }

    entry fun t06() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            'b: { continue 'a }
        };
    }

    entry fun t07() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            ('b: { continue 'a } : ())
        };
    }

    entry fun t08() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            'b: { break 'a }
        };
    }

    entry fun t09() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            ('b: { break 'a } : ())
        };
    }

    entry fun t10() {
        loop 'a: {
            'b: {
                'c: { loop 'd: { break 'a } }
            }
        }
    }

    entry fun t11() {
        let _x = loop 'a: {
            (loop { break 'a 0 } : ())
        };
    }

    entry fun t12() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            (loop { continue 'a } : ())
        }
    }

    entry fun t13() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            'b: { continue 'a }
        }
    }

    entry fun t14() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            ('b: { continue 'a } : ())
        }
    }

    entry fun t15() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            'b: { break 'a }
        }
    }

    entry fun t16() {
        let mut i = 0;
        while (i < 10) 'a: {
            i = i + 1;
            ('b: { break 'a } : ())
        }
    }

    entry fun t17() {
        loop 'a: {
            ('b: {
                'c: { loop 'd: { break 'a } }
            }: ())
        }
    }

    entry fun t18() {
        loop 'a: {
            'b: {
                ('c: { loop 'd: { break 'a } } : ())
            }
        }
    }

    entry fun t19() {
        loop 'a: {
            ('b: {
                 ('c: { loop 'd: { break 'a } } : ())
            } : ())
        }
    }

    entry fun t20(): u64 {
        loop 'a: {
            ('b: {
                'c: { loop 'd: { break 'a 0 } }
            }: ())
        }
    }

    entry fun t21(): u64 {
        loop 'a: {
            'b: {
                ('c: { loop 'd: { break 'a 0 } } : ())
            }
        }
    }

    entry fun t22(): u64 {
        loop 'a: {
            ('b: {
                 ('c: { loop 'd: { break 'a 0 } } : ())
            } : ())
        }
    }

    entry fun t23(): u64 {
        let x = loop 'a: {
            ('b: {
                'c: { loop 'd: { break 'a 0 } }
            }: ())
        };
        x
    }

    entry fun t24(): u64 {
        let x = loop 'a: {
            'b: {
                ('c: { loop 'd: { break 'a 0 } } : ())
            }
        };
        x
    }

    entry fun t25(): u64 {
        let x = loop 'a: {
            ('b: {
                 ('c: { loop 'd: { break 'a 0 } } : ())
            } : ())
        };
        x
    }
}

//# run 42::m::t00

//# run 42::m::t01

//# run 42::m::t02

//# run 42::m::t03

//# run 42::m::t04

//# run 42::m::t05

//# run 42::m::t06

//# run 42::m::t07

//# run 42::m::t08

//# run 42::m::t09

//# run 42::m::t10

//# run 42::m::t11

//# run 42::m::t12

//# run 42::m::t13

//# run 42::m::t14

//# run 42::m::t15

//# run 42::m::t16

//# run 42::m::t17

//# run 42::m::t18

//# run 42::m::t19

//# run 42::m::t20

//# run 42::m::t21

//# run 42::m::t22

//# run 42::m::t23

//# run 42::m::t24

//# run 42::m::t25

