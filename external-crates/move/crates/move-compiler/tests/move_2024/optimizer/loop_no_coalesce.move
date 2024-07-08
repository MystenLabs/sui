module a::m;

public struct X { }

fun f() {
   let mut _y;
   let mut _x = X { };
   let _z = _x;
   loop {
       _y = X { };
       let _q = _y;
       abort 0
   }
}
