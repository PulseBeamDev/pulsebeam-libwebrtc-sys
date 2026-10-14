//! Containment for user code called by native provider adapters.

/// Erase a caught panic without running an untrusted payload destructor.
/// Even catching a second panic from `drop(payload)` is insufficient: its
/// field cleanup can panic during that unwind and abort before the catch.
/// Known string payloads can be freed; arbitrary payloads must be forgotten.
pub(crate) fn catch_provider<T>(call: impl FnOnce() -> T) -> Result<T, ()> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)) {
        Ok(value) => Ok(value),
        Err(payload) => {
            if payload.is::<String>() || payload.is::<&'static str>() {
                drop(payload);
            } else {
                std::mem::forget(payload);
            }
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::catch_provider;

    struct Field;
    impl Drop for Field {
        fn drop(&mut self) {
            panic!("panic during payload field cleanup");
        }
    }
    struct Payload(Field);
    impl Drop for Payload {
        fn drop(&mut self) {
            panic!("panic in payload destructor");
        }
    }

    #[test]
    fn contains_strings_and_payloads_whose_cleanup_would_double_panic() {
        assert_eq!(catch_provider(|| 42), Ok(42));
        assert!(catch_provider(|| panic!("string literal")).is_err());
        assert!(catch_provider(|| std::panic::panic_any(String::from("owned string"))).is_err());
        assert!(catch_provider(|| std::panic::panic_any(Payload(Field))).is_err());
    }
}
