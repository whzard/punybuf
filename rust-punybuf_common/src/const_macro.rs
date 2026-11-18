macro_rules! const_expect {
	($e:expr, $msg:expr $(,)?) => {
		match $e {
			Ok(x) => x,
			Err(_) => panic!($msg),
		}
	};
}

pub(crate) use const_expect;