use std::path::Path;

use msbuild::MsBuild;

fn main() {
    let mb = MsBuild::find_msbuild(Some("2017"));
    match mb {
        Ok(msb) => {
            let _ = msb.run(Path::new("./"), &[]);
            println!("Found msbuild");
        }
        Err(_) => {
            println!("Failed to find msbuild");
        }
    }
}
