#[macro_export]
macro_rules! const_expect {
	($e:expr, $msg:expr $(,)?) => {
		match $e {
			Ok(x) => x,
			Err(_) => panic!($msg),
		}
	};
}