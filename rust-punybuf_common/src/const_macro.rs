#[macro_export]
macro_rules! const_unwrap {
    ($e:expr $(,)?) => {
        match $e {
            Ok(x) => x,
            Err(_) => panic!("invalid env variables"),
        }
    };
}