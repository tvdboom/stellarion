use super::*;

#[test]
/// Resource arithmetic saturates instead of wrapping below zero or above the platform limit.
fn resource_arithmetic_never_wraps() {
    assert_eq!(Resources::new(0, 1, 2) - Resources::new(1, 2, 3), Resources::default());
    assert_eq!(
        Resources::new(usize::MAX, usize::MAX, usize::MAX) + 1_usize,
        Resources::new(usize::MAX, usize::MAX, usize::MAX)
    );
}
