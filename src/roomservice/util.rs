use scribe_rust::log;
use std::fmt::Display;

pub fn fail<T: Display>(message: T) {
    log(scribe_rust::Color::Red, "Error", &message.to_string());
    std::process::exit(1)
}

pub trait Failable<T> {
    fn unwrap_fail(self, message: &str) -> T;
}

impl<T> Failable<T> for Option<T> {
    fn unwrap_fail(self, message: &str) -> T {
        match self {
            Some(unwrapped) => unwrapped,
            None => {
                fail(message);
                unreachable!()
            }
        }
    }
}

impl<T, E> Failable<T> for Result<T, E> {
    fn unwrap_fail(self, message: &str) -> T {
        match self {
            Ok(unwrapped) => unwrapped,
            Err(_) => {
                fail(message);
                unreachable!()
            }
        }
    }
}
