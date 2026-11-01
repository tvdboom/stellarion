use super::*;

#[test]
fn loot_shares_preserve_each_resource_and_respect_each_carriers_capacity() {
    for weights in [vec![10, 30, 60], vec![0, 100, 0], vec![1; 100]] {
        for resources in [
            Resources::new(12, 23, 34),
            Resources::new(1, 1, 98),
            Resources::new(0, 1, 0),
            Resources::default(),
        ] {
            let shares = resource_shares(resources, &weights);
            assert_eq!(shares.iter().copied().sum::<Resources>(), resources);
            for (share, capacity) in shares.iter().zip(&weights) {
                assert!(share.total() <= *capacity);
            }
        }
    }
    assert_eq!(resource_shares(Resources::new(2, 3, 4), &[0, 0]), vec![Resources::default(); 2]);
    assert!(resource_shares(Resources::default(), &[]).is_empty());
}

#[test]
fn loot_shares_keep_large_resource_counts_without_multiplication_overflow() {
    let resources = Resources::new(usize::MAX / 3, usize::MAX / 3, usize::MAX / 3);
    let shares = resource_shares(resources, &[usize::MAX / 2, usize::MAX / 2, 1]);
    assert_eq!(shares.iter().copied().sum::<Resources>(), resources);
}

#[test]
fn raiding_uses_a_distinct_quiet_cue_from_crawler_recovery() {
    let raid = resource_pickup_cue(true);
    let salvage = resource_pickup_cue(false);
    assert_eq!(raid.name, "raid loot");
    assert_eq!(salvage.name, "construction");
    assert!(raid.volume < 0.0);
    assert!(!raid.is_looped);
}
