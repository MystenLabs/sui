module a::m;

public struct X { x: u64 }

fun f() {
   let mut _y;
   let mut _x = X { x: 0 };
   _x.x = 1;
   let _z = _x;
   loop {
       _y = X { x: 0 };
        _y.x = 1;
       let _q = _y;
       abort 0
   }
}
