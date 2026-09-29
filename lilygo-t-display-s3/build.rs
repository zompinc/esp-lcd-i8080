fn main() {
    // Examples are firmware images; the library itself links into the user's binary.
    println!("cargo:rustc-link-arg-examples=-Tlinkall.x");
}
