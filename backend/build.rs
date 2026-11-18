use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-env-changed=SEVER_REQUIRE_REACT");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let dist = PathBuf::from("../frontend/dist");
    let bundle = dist.join("sever-ui.js");
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-changed={}", bundle.display());

    let javascript = match fs::read(&bundle) {
        Ok(javascript) => javascript,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    if javascript.is_empty() && env::var("SEVER_REQUIRE_REACT").as_deref() == Ok("1") {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "React bundle is missing. Run npm --prefix frontend run build:server.",
        ));
    }
    fs::write(output.join("sever-ui.js"), javascript)
}
