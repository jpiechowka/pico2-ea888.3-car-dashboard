use dashboard_host_validation::stack_samples::StackSamples;

#[test]
fn stack_samples_are_checked_and_only_an_observed_lower_bound() {
    assert!(StackSamples::new(100, 99).is_none());
    assert!(StackSamples::new(100, 100).is_none());
    let mut stack = StackSamples::new(100, 200).unwrap();
    assert_eq!(stack.observe(200), Some(0));
    assert_eq!(stack.observe(150), Some(50));
    assert_eq!(stack.observe(190), Some(50));
    assert_eq!(stack.observe(99), None);
    assert_eq!(stack.observe(201), None);
    assert_eq!(stack.observe(120), Some(80));
    assert_eq!(stack.observe(100), Some(100));
    let mut huge = StackSamples::new(0, u32::MAX).unwrap();
    assert_eq!(huge.observe(0), Some(u32::MAX));
}
