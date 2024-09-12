//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Temperature {
        Fahrenheit(u64),
        Celcius(u32)
    }

    public fun f(): Temperature {
        Temperature::Fahrenheit(32)
    }

    public fun c(): Temperature {
        Temperature::Celcius(0)
    }

    public fun dtor(t: Temperature) {
        match (t) {
            Temperature::Fahrenheit(_) => (), 
            Temperature::Celcius(_) => (), 
        }
    }

    public fun is_temperature_fahrenheit(t: &Temperature): bool {
       match (t) {
          Temperature::Fahrenheit(_) => true,
          _ => false,
       }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        let f = m::f();
        let c = m::c();
        assert!(f.is_temperature_fahrenheit());
        assert!(!c.is_temperature_fahrenheit());

        m::dtor(f);
        m::dtor(c);
    }
}
