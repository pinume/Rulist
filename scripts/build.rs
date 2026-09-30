use std::{env, path::Path};

fn main() {
    let frontend = Path::new("public/dist/index.html");
    println!("cargo:rerun-if-changed={}", frontend.display());

    if env::var("PROFILE").as_deref() == Ok("release") && !frontend.is_file() {
        panic!("release builds require the frontend; run ./scripts/build-release.sh");
    }
}
