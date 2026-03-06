//! Module that contains functionality for programtically
//! retrieve information about the windows SDKs available on
//! the system.
use lenient_semver::Version;
use std::{
    collections::BTreeMap,
    fs::DirEntry,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

/// Struct holding information regarding the include
/// paths of the windows SDK.
#[derive(Debug)]
pub struct WinSdkIncludes {
    cppwinrt: PathBuf,
    shared: PathBuf,
    ucrt: PathBuf,
    um: PathBuf,
    winrt: PathBuf,
}

impl WinSdkIncludes {
    const CPPWINRT_DIR: &'static str = "cppwinrt";
    const SHARED_DIR: &'static str = "shared";
    const UCRT_DIR: &'static str = "ucrt";
    const UM_DIR: &'static str = "um";
    const WINRT_DIR: &'static str = "winrt";
    const EXPECTED_DIRS: [&'static str; 5] = [
        Self::CPPWINRT_DIR,
        Self::SHARED_DIR,
        Self::UCRT_DIR,
        Self::UM_DIR,
        Self::WINRT_DIR,
    ];

    /// Creates a WinSdkInclude object from include path.
    pub fn create(include_path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            cppwinrt: sub_directory(include_path, Self::CPPWINRT_DIR)?,
            shared: sub_directory(include_path, Self::SHARED_DIR)?,
            ucrt: sub_directory(include_path, Self::UCRT_DIR)?,
            um: sub_directory(include_path, Self::UM_DIR)?,
            winrt: sub_directory(include_path, Self::WINRT_DIR)?,
        })
    }

    pub fn cppwinrt_dir(&self) -> &Path {
        self.cppwinrt.as_path()
    }

    pub fn shared_dir(&self) -> &Path {
        self.shared.as_path()
    }

    pub fn ucrt_dir(&self) -> &Path {
        self.ucrt.as_path()
    }

    pub fn um_dir(&self) -> &Path {
        self.um.as_path()
    }

    pub fn winrt_dir(&self) -> &Path {
        self.winrt.as_path()
    }

    pub fn is_valid(path: &Path) -> bool {
        // This should probably include some kind of trace logging
        // explainin why the dir was not valid.
        path.is_dir() && !Self::EXPECTED_DIRS.iter().any(|s| !path.join(s).is_dir())
    }
}

/// The windows SDK version.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct WinSdkVersion<'a>(Version<'a>);

impl<'a> WinSdkVersion<'a> {
    pub fn parse(value: &'a str) -> std::io::Result<WinSdkVersion<'a>> {
        Version::parse(value).map_or_else(
            |e| {
                Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse &str as a WinSdkVersion: {}", e),
                ))
            },
            |v| Ok(WinSdkVersion(v)),
        )
    }
}

/// Struct holding information regarding the Windows SDK.
pub struct WinSdk {
    include: WinSdkIncludes,
}

impl WinSdk {
    const ENV_KEY: &'static str = "WIN_SDK_PATH";
    const REG_PATH: &'static str =
        "SOFTWARE\\WOW6432Node\\Microsoft\\Microsoft SDKs\\Windows\\v10.0";
    const HKLM: winreg::RegKey = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);

    pub const fn include_dirs(&self) -> &WinSdkIncludes {
        &self.include
    }

    // Finds a Windows SDK.
    pub fn find() -> std::io::Result<Self> {
        Self::find_in_range(None, None)
    }

    /// Finds a Windows SDK in the specified version range.
    pub fn find_in_range(
        max: Option<WinSdkVersion>,
        min: Option<WinSdkVersion>,
    ) -> std::io::Result<Self> {
        // Each folder of intresst conatins folders with a version as the name.
        // If other folders are of interesst then the versions must match.
        // |-- Include
        // |    |-- 10.0.a.0
        // |    |-- 10.0.b.0
        // |-- Lib
        // |    |-- 10.0.a.0
        // In the case above the only option would be 10.0.a.0 and if that version
        // is not in the version range then no WinSdk would be found.
        let installation_folder = Self::installation_folder()?;
        let include_versioned_dirs = Self::include_versioned_subdirs(
            installation_folder.as_path(),
            max.as_ref(),
            min.as_ref(),
        )?;

        Self::select_sdk(include_versioned_dirs)
    }

    // Checks the version in all the interessting directories and selects
    // the latest common version.
    fn select_sdk(versioned_include_dirs: Vec<PathBuf>) -> std::io::Result<Self> {
        let versioned_include_dirs_map =
            Self::versioned_directory_map(versioned_include_dirs.as_slice());
        // Unwrap is safe here the map cannot be empty.
        let (_, d) = versioned_include_dirs_map.last_key_value().unwrap();

        Ok(Self {
            include: WinSdkIncludes::create(d.as_path())?,
        })
    }

    /// Creates a map that maps SDK versions to directories.
    fn versioned_directory_map(version_dirs: &[PathBuf]) -> BTreeMap<WinSdkVersion<'_>, &PathBuf> {
        version_dirs
            .iter()
            .map(|d| {
                // It is ok to unwrap version_dirs are expected to only
                // contain dirs that can be parsed.
                let v = d
                    .file_name()
                    .and_then(|o| o.to_str())
                    .and_then(|s| WinSdkVersion::parse(s).ok())
                    .unwrap();
                (v, d)
            })
            .collect::<BTreeMap<WinSdkVersion, &PathBuf>>()
    }

    /// Collects all the versioned Include directories.
    fn include_versioned_subdirs(
        parent: &Path,
        max: Option<&WinSdkVersion>,
        min: Option<&WinSdkVersion>,
    ) -> std::io::Result<Vec<PathBuf>> {
        let search_dir = sub_directory(parent, "Include")?;
        // Filter out Paths that are not dirs
        // and Paths where the ending cannot be parsed
        // as WinSdkVersion.
        let found = search_dir
            .read_dir()?
            .filter_map(|r| r.ok())
            .filter_map(Self::as_valid_path)
            .filter(|path| Self::is_valid_versioned_subdir(path, max, min))
            .filter(|path| WinSdkIncludes::is_valid(path))
            .collect::<Vec<PathBuf>>();
        if found.is_empty() {
            return Err(Error::new(
            ErrorKind::NotFound,
            format!("No versioned `Include` directories in the specified version range were found inside `{}` dir.", search_dir.to_string_lossy()),
        ));
        }
        Ok(found)
    }

    // Turns a DirEntry into a PathBuf object if it is an existing directory.
    fn as_valid_path(de: DirEntry) -> Option<PathBuf> {
        let path = de.path();
        if !path.is_dir() {
            return None;
        }
        Some(path)
    }

    // Checks that path is a versioned sub dir in the specified range.
    fn is_valid_versioned_subdir(
        path: &Path,
        max: Option<&WinSdkVersion>,
        min: Option<&WinSdkVersion>,
    ) -> bool {
        path.file_name()
            .and_then(|ver_dir| ver_dir.to_str())
            .and_then(|ver_dir_str| WinSdkVersion::parse(ver_dir_str).ok())
            .is_some_and(|win_sdk_ver| Self::has_version_in_range(&win_sdk_ver, max, min))
    }

    fn installation_folder() -> std::io::Result<PathBuf> {
        Self::installation_folder_environment_variable()
            .unwrap_or_else(Self::installation_folder_from_registry)
    }

    /// Extracts the installation folder from the environment variable.
    fn installation_folder_environment_variable() -> Option<std::io::Result<PathBuf>> {
        std::env::var(WinSdk::ENV_KEY).ok().map(|s| {
            let path = PathBuf::from(s);
            if !path.is_dir() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "`WIN_SDK_PATH` environment variable contained invalid data.",
                ));
            }
            Ok(path)
        })
    }

    /// Extracts the installation folder from the Windows registry.
    fn installation_folder_from_registry() -> std::io::Result<PathBuf> {
        Self::HKLM
            .open_subkey(Self::REG_PATH)
            .and_then(|sdk_entry| sdk_entry.get_value("InstallationFolder"))
            .and_then(|path_string: String| {
                let path = Path::new(path_string.as_str());
                if !path.is_dir() {
                    return Err(Error::new(
                        ErrorKind::NotFound,
                        format!(
                            "The InstallationFolder `{}` does not exist.",
                            path_string.as_str()
                        ),
                    ));
                }
                Ok(PathBuf::from(path_string))
            })
    }

    /// Internal function to check if a version is in the range
    /// if it has been specified.
    fn has_version_in_range(
        version: &WinSdkVersion,
        max: Option<&WinSdkVersion>,
        min: Option<&WinSdkVersion>,
    ) -> bool {
        let is_below_max: bool = max.is_none_or(|max_version| max_version > version);
        let is_above_min: bool = min.is_none_or(|min_version| version >= min_version);
        is_below_max && is_above_min
    }
}

/// Constructs a verified object representing the path to the sub directory.
fn sub_directory(parent: &Path, dir: &str) -> std::io::Result<PathBuf> {
    let sub_dir = parent.join(dir);
    if !sub_dir.is_dir() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "{} does not contain the {} directory.",
                parent.to_string_lossy(),
                dir
            ),
        ));
    }
    Ok(sub_dir)
}

// ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Unit tests of the private functions and methods
// ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod test {
    use super::*;
    use std::collections::BTreeSet;
    use tempfile::tempdir;

    #[test]
    fn test_win_sdk_includes_create_error() {
        let invalid_path = PathBuf::from(".");
        let error = WinSdkIncludes::create(&invalid_path).expect_err(
            "Creating a WinSdkIncludes object with an invalid path should result in an error.",
        );
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_win_sdk_includes() {
        let temp_dir = tempdir().expect("It should be possible to create a temporary directory.");

        let is_not_valid = !WinSdkIncludes::is_valid(temp_dir.path());

        assert!(is_not_valid);

        WinSdkIncludes::EXPECTED_DIRS.iter().for_each(|s| {
            std::fs::create_dir(temp_dir.path().join(s))
                .unwrap_or_else(|_| panic!("It should be possible to create the dir {}", s))
        });

        assert!(WinSdkIncludes::is_valid(temp_dir.path()));

        let actual = WinSdkIncludes::create(temp_dir.path())
            .expect("It should be possible to create a WinSdkIncludes object when all sub directories are present.");

        assert_eq!(
            actual.cppwinrt_dir(),
            temp_dir.path().join(WinSdkIncludes::CPPWINRT_DIR).as_path()
        );
        assert_eq!(
            actual.shared_dir(),
            temp_dir.path().join(WinSdkIncludes::SHARED_DIR).as_path()
        );
        assert_eq!(
            actual.ucrt_dir(),
            temp_dir.path().join(WinSdkIncludes::UCRT_DIR).as_path()
        );
        assert_eq!(
            actual.um_dir(),
            temp_dir.path().join(WinSdkIncludes::UM_DIR).as_path()
        );
        assert_eq!(
            actual.winrt_dir(),
            temp_dir.path().join(WinSdkIncludes::WINRT_DIR).as_path()
        );
    }

    #[test]
    fn test_include_versioned_subdirs() {
        // tmp
        //  |-> include
        //    |-> 10.0.2.0
        //    |-> 10.0.1.0
        //    |-> 10.0.0.0
        let temp_dir = tempdir().expect("It should be possible to create a temporary directory.");
        let parent = temp_dir.path();
        let include_dir = temp_dir.path().join("Include");
        std::fs::create_dir(include_dir.as_path()).expect(
            "It should be possible to create a `Include` folder inside the temproary directory",
        );

        let expected = ["10.0.2.0", "10.0.1.0", "10.0.0.0"]
            .iter()
            .map(|v| {
               let versioned_subdir = include_dir.as_path().join(v);
               std::fs::create_dir(versioned_subdir.as_path()).expect("It should be possible to create a versioned sub dir folder inside the `Include` directory");
               WinSdkIncludes::EXPECTED_DIRS.iter().for_each(|s| {
                   std::fs::create_dir(versioned_subdir.as_path().join(s))
                       .unwrap_or_else(|_| panic!("It should be possible to create the dir {}", s))
               });
               versioned_subdir
            })
            .collect::<BTreeSet<PathBuf>>();
        let actual = WinSdk::include_versioned_subdirs(parent, None, None)
            .expect(
                "It should be possible to find a valid include sub directory in the parent folder.",
            )
            .into_iter()
            .collect::<BTreeSet<PathBuf>>();

        assert_eq!(expected, actual);
    }
}
