pub fn pass(label: &str) {
    println!("{label:.<52} PASS");
}

pub fn fail(label: &str) {
    println!("{label:.<52} FAIL");
}

pub fn stage(current: usize, total: usize, feature: &str) {
    println!("\n[{current}/{total}] Inserting {feature}");
}
