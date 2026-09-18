use super::*;

const _: () = assert!(AFTERMATH_LABEL_EXTENSION_SECONDS >= 1.0);

#[test]
fn aftermath_labels_rise_continuously_through_their_fade() {
    let base_y = 80.0;
    let sample = |elapsed| aftermath_label_motion(base_y, elapsed, 1.0, 5.0, 0.5, 1.0);

    assert_eq!(sample(0.5), (base_y, 0.0));

    let appearing = sample(1.25);
    let visible = sample(2.0);
    let disappearing = sample(4.5);
    let finished = sample(5.0);

    assert!((0.0..1.0).contains(&appearing.1));
    assert_eq!(visible.1, 1.0);
    assert!((0.0..1.0).contains(&disappearing.1));
    assert!(base_y < appearing.0 && appearing.0 < visible.0);
    assert!(visible.0 < disappearing.0 && disappearing.0 < finished.0);
    assert_eq!(finished, (base_y + AFTERMATH_LABEL_RISE, 0.0));
}
