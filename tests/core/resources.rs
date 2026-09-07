use super::*;

#[test]
fn percentage_scaling_preserves_large_balances_and_saturates_after_division() {
    let resources = Resources::new(usize::MAX, 101, 0);
    assert_eq!(resources.scaled_percent(100), resources);
    assert_eq!(resources.scaled_percent(0), Resources::default());
    assert_eq!(resources.scaled_percent(50), Resources::new(usize::MAX / 2, 50, 0));
    assert_eq!(resources.scaled_percent(200), Resources::new(usize::MAX, 202, 0));
    assert_eq!(
        Resources::new(1, 1, 1).scaled_percent(usize::MAX),
        Resources::new(usize::MAX / 100, usize::MAX / 100, usize::MAX / 100)
    );
}

#[test]
/// Resource arithmetic saturates instead of wrapping below zero or above the platform limit.
fn resource_arithmetic_never_wraps() {
    assert_eq!(Resources::new(0, 1, 2) - Resources::new(1, 2, 3), Resources::default());
    assert_eq!(
        Resources::new(usize::MAX, usize::MAX, usize::MAX) + 1_usize,
        Resources::new(usize::MAX, usize::MAX, usize::MAX)
    );
}
