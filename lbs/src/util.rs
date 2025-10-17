use libc::*;

#[macro_export]
macro_rules! replace_expr {
    ($e:expr, $with:expr) => {
        $with
    };
}

#[macro_export]
macro_rules! count_exprs {
    ($($e:expr),*) => {
        0 $(+ replace_expr!($e, 1))*
    };
}

#[macro_export]
macro_rules! assert_implies {
    ($lhs:expr, $rhs:expr) => {
        assert!(if $lhs { $rhs } else { true });
    };
}

#[macro_export]
macro_rules! addr {
    ($e:expr) => {
        std::ptr::addr_of_mut!($e)
    };
}

pub const NULL: *mut c_void = std::ptr::null_mut();

