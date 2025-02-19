use std::time::Duration;

use criterion::{
    measurement::{Measurement, ValueFormatter},
    Throughput,
};
use libc::{c_long, rusage, suseconds_t, time_t, timespec, timeval};

pub enum PosixTime {
    UserTime,
    UserAndSystemTime,
}
impl Measurement for PosixTime {
    type Intermediate = Duration;
    type Value = Duration;

    fn start(&self) -> Self::Intermediate {
        self.get_time()
    }

    fn end(&self, i: Self::Intermediate) -> Self::Value {
        self.get_time() - i
    }

    fn add(&self, v1: &Self::Value, v2: &Self::Value) -> Self::Value {
        *v1 + *v2
    }

    fn zero(&self) -> Self::Value {
        Duration::from_secs(0)
    }

    fn to_f64(&self, value: &Self::Value) -> f64 {
        value.as_nanos() as f64
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        &DurationFormatter
    }
}

fn get_r_usage() -> Result<Box<rusage>, libc::c_int> {
    let mut r_usage = rusage {
        ru_utime: timeval {
            tv_sec: 0 as time_t,
            tv_usec: 0 as suseconds_t,
        },
        ru_stime: timeval {
            tv_sec: 0 as time_t,
            tv_usec: 0 as suseconds_t,
        },
        ru_maxrss: 0 as c_long,
        ru_ixrss: 0 as c_long,
        ru_idrss: 0 as c_long,
        ru_isrss: 0 as c_long,
        ru_minflt: 0 as c_long,
        ru_majflt: 0 as c_long,
        ru_nswap: 0 as c_long,
        ru_inblock: 0 as c_long,
        ru_oublock: 0 as c_long,
        ru_msgsnd: 0 as c_long,
        ru_msgrcv: 0 as c_long,
        ru_nsignals: 0 as c_long,
        ru_nvcsw: 0 as c_long,
        ru_nivcsw: 0 as c_long,
    };

    let errno = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r_usage as *mut rusage) };

    if errno != 0 {
        Err(errno)
    } else {
        Ok(Box::new(r_usage))
    }
}

fn clock_gettime() -> Result<Box<timespec>, libc::c_int> {
    let mut time_spec = timespec {
        tv_nsec: 0 as c_long,
        tv_sec: 0 as time_t,
    };
    let errno = unsafe {
        libc::clock_gettime(
            libc::CLOCK_PROCESS_CPUTIME_ID,
            &mut time_spec as *mut timespec,
        )
    };
    if errno != 0 {
        Err(errno)
    } else {
        Ok(Box::new(time_spec))
    }
}

impl PosixTime {
    pub(crate) fn get_time(&self) -> Duration {
        match self {
            PosixTime::UserTime => {
                let r_usage = get_r_usage();
                match r_usage {
                    Ok(r_usage) => {
                        Duration::from_micros(r_usage.ru_utime.tv_usec as u64)
                            + Duration::from_secs(r_usage.ru_utime.tv_sec as u64)
                    }
                    Err(errno) => panic!("getrusage() error: {}", errno),
                }
            }
            PosixTime::UserAndSystemTime => {
                let time_spec = clock_gettime();
                match time_spec {
                    Ok(time_spec) => {
                        Duration::from_secs(time_spec.tv_sec as u64)
                            + Duration::from_nanos(time_spec.tv_nsec as u64)
                    }
                    Err(errno) => panic!("clock_gettime() error: {}", errno),
                }
            }
        }
    }
}

pub(crate) struct DurationFormatter;
impl DurationFormatter {
    fn bytes_per_second(&self, bytes: f64, typical: f64, values: &mut [f64]) -> &'static str {
        let bytes_per_second = bytes * (1e9 / typical);
        let (denominator, unit) = if bytes_per_second < 1024.0 {
            (1.0, "  B/s")
        } else if bytes_per_second < 1024.0 * 1024.0 {
            (1024.0, "KiB/s")
        } else if bytes_per_second < 1024.0 * 1024.0 * 1024.0 {
            (1024.0 * 1024.0, "MiB/s")
        } else {
            (1024.0 * 1024.0 * 1024.0, "GiB/s")
        };

        for val in values {
            let bytes_per_second = bytes * (1e9 / *val);
            *val = bytes_per_second / denominator;
        }

        unit
    }

    fn bytes_decimal_per_second(
        &self,
        bytes: f64,
        typical: f64,
        values: &mut [f64],
    ) -> &'static str {
        let bytes_per_second = bytes * (1e9 / typical);
        let (denominator, unit) = if bytes_per_second < 1000.0 {
            (1.0, "  B/s")
        } else if bytes_per_second < 1000.0 * 1000.0 {
            (1000.0, "KiB/s")
        } else if bytes_per_second < 1000.0 * 1000.0 * 1000.0 {
            (1000.0 * 1000.0, "MiB/s")
        } else {
            (1000.0 * 1000.0 * 1000.0, "GiB/s")
        };

        for val in values {
            let bytes_per_second = bytes * (1e9 / *val);
            *val = bytes_per_second / denominator;
        }

        unit
    }

    fn elements_per_second(&self, elems: f64, typical: f64, values: &mut [f64]) -> &'static str {
        let elems_per_second = elems * (1e9 / typical);
        let (denominator, unit) = if elems_per_second < 1000.0 {
            (1.0, " elem/s")
        } else if elems_per_second < 1000.0 * 1000.0 {
            (1000.0, "Kelem/s")
        } else if elems_per_second < 1000.0 * 1000.0 * 1000.0 {
            (1000.0 * 1000.0, "Melem/s")
        } else {
            (1000.0 * 1000.0 * 1000.0, "Gelem/s")
        };

        for val in values {
            let elems_per_second = elems * (1e9 / *val);
            *val = elems_per_second / denominator;
        }

        unit
    }
}

impl ValueFormatter for DurationFormatter {
    fn scale_values(&self, ns: f64, values: &mut [f64]) -> &'static str {
        let (factor, unit) = if ns < 10f64.powi(0) {
            (10f64.powi(3), "ps")
        } else if ns < 10f64.powi(3) {
            (10f64.powi(0), "ns")
        } else if ns < 10f64.powi(6) {
            (10f64.powi(-3), "us")
        } else if ns < 10f64.powi(9) {
            (10f64.powi(-6), "ms")
        } else {
            (10f64.powi(-9), "s")
        };

        for val in values {
            *val *= factor;
        }

        unit
    }

    fn scale_throughputs(
        &self,
        typical: f64,
        throughput: &Throughput,
        values: &mut [f64],
    ) -> &'static str {
        match *throughput {
            Throughput::Bytes(bytes) => self.bytes_per_second(bytes as f64, typical, values),
            Throughput::Elements(elems) => self.elements_per_second(elems as f64, typical, values),
            Throughput::BytesDecimal(bytes) => {
                self.bytes_decimal_per_second(bytes as f64, typical, values)
            }
        }
    }

    fn scale_for_machines(&self, _values: &mut [f64]) -> &'static str {
        // no scaling is needed
        "ns"
    }
}
