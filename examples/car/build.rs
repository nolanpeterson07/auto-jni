use std::env;
use std::path::Path;
use auto_jni::Builder;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out = env::var("OUT_DIR").unwrap();
    let file = Path::new(&out).join("bindings.rs");

    Builder::new()
        .class("com.example.Car")
        .class_path("../java/src")
        .jvm_option("-Djava.class.path=../java/src")
        .generate(&file)
        .expect("Failed to generate bindings");
}
