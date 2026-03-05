//! Module that contains the code for providing the
//! the `VsWhere.exe` binary functionality.
use std::{
    io::{Error, ErrorKind},
    path::PathBuf,
};

/// Type for finding and interacting with the
/// vswhere executable.
pub struct VsWhere {
    path: PathBuf,
}

impl VsWhere {
    const DEFAULT_PATH: &'static str =
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe";
    const ENV_KEY: &'static str = "VS_WHERE_PATH";
    const DEFAULT_ARGS: [&'static str; 7] = [
        "-legacy",
        "-prerelease",
        "-format",
        "json",
        "-utf8",
        "-products",
        "*",
    ];

    /// Creates a VsWhere object if the `vswhere.exe`binary can be found.
    pub fn find_vswhere() -> std::io::Result<Self> {
        let path: PathBuf = VsWhere::vswhere_path();
        if path.exists() {
            Ok(VsWhere { path })
        } else {
            Err(Error::new(
                ErrorKind::NotFound,
                format!("The path [{}] does not exists.", path.to_string_lossy()),
            ))
        }
    }

    /// Runs the executable with the provided argument
    /// or default argument if no arguments are provided.
    pub fn run(self, args: Option<&[&str]>) -> std::io::Result<String> {
        let command_args: &[&str] = args.unwrap_or(VsWhere::DEFAULT_ARGS.as_ref());
        std::process::Command::new(self.path)
            .args(command_args)
            .output()
            .and_then(|output: std::process::Output| {
                std::str::from_utf8(&output.stdout).map_or_else(
                    |e: std::str::Utf8Error| {
                        Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Command output could not be parsed as UTF-8 ({}).", e),
                        ))
                    },
                    |v: &str| Ok(v.to_string()),
                )
            })
    }

    fn vswhere_path() -> PathBuf {
        PathBuf::from(std::env::var(VsWhere::ENV_KEY).unwrap_or(VsWhere::DEFAULT_PATH.to_string()))
    }
}

// ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Unit tests of the private functions and methods
// ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod test {
    #[ignore]
    #[test]
    fn test_vswhere_find_vswhere_internal() {
        // Cannot run the tests unless vswhere has
        // been installed into the test environment.
    }
}
