module A::l;

use A::m as am;

#[allow(unused_function)]
fun f() {
    am::make_bar();
    am::verify_fun();
    l<am::Bar>();
    verify_fun();
}

#[allow(unused_function, unused_type_parameter)]
fun l<T>() { }

#[verify_only]
fun verify_fun() {  }
