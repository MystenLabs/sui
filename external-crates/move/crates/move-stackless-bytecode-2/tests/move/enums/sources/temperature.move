// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module enums::temperature {

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