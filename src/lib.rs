//! # The msbuild crate
//! This crates provides the functionality of finding
//! the msbuild binary on the system.
//!
//! # Environment Variables
//! - The `VS_WHERE_PATH` environment variable can be used in order
//!   overwrite the default path where the crate tries to locate
//!   the `vswhere.exe` binary.
//!
//! - The `VS_INSTALLATION_PATH` environment variable can be used in order
//!   to overwrite specify a path to Visual Studio
//!   Note! The path must still lead to a binary the fulfills the version
//!   requirements otherwise the crate will try to probe the system
//!   for a suitable version.
//!
//! - The `WIN_SDK_PATH` environment variable can be used in order to
//!   to overwrite in what location the library will search for
//!   WinSDK installations.
use lenient_semver::Version;
use serde_json::Value;
use std::{
    convert::TryFrom,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

pub mod vs_where;
pub mod win_sdk;

pub use vs_where::VsWhere;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct InstallationVersion<'a>(Version<'a>);

impl<'a> InstallationVersion<'a> {
    pub fn parse(value: &'a str) -> std::io::Result<InstallationVersion<'a>> {
        Version::parse(value).map_or_else(
            |e| {
                Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse &str as a InstallationVersion: {}", e),
                ))
            },
            |v| Ok(InstallationVersion(v)),
        )
    }
}

/// Enum holding the product line versions.
pub enum ProductLineVersion {
    Vs2022,
    Vs2019,
    Vs2017,
}

impl ProductLineVersion {
    /// The non inclusive max installation version for a
    /// specific product line version.
    pub fn installation_version_max(&self) -> InstallationVersion<'_> {
        // Constant values that are always safe to parse.
        match self {
            Self::Vs2022 => InstallationVersion::parse("18.0.0.0").unwrap(),
            Self::Vs2019 => InstallationVersion::parse("17.0.0.0").unwrap(),
            Self::Vs2017 => InstallationVersion::parse("16.0.0.0").unwrap(),
        }
    }

    /// The inclusive min installation version for a
    /// specific product line version.
    pub fn installation_version_min(&self) -> InstallationVersion<'_> {
        match self {
            Self::Vs2022 => InstallationVersion::parse("17.0.0.0").unwrap(),
            Self::Vs2019 => InstallationVersion::parse("16.0.0.0").unwrap(),
            Self::Vs2017 => InstallationVersion::parse("15.0.0.0").unwrap(),
        }
    }
}

impl TryFrom<&str> for ProductLineVersion {
    type Error = Error;

    fn try_from(s: &str) -> std::io::Result<Self> {
        match s {
            "2017" => Ok(ProductLineVersion::Vs2017),
            "2019" => Ok(ProductLineVersion::Vs2019),
            "2022" => Ok(ProductLineVersion::Vs2022),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Product line version {} did not match any known values.", s),
            )),
        }
    }
}

/// Type for finding and interactive with
/// the msbuild executable.
pub struct MsBuild {
    path: PathBuf,
}

impl MsBuild {
    const ENV_KEY: &'static str = "VS_INSTALLATION_PATH";
    /// Finds the msbuild executable that is associated with provided product line version
    /// if no version is provided then the first installation of msbuild that is found
    /// will be selected.
    ///
    /// # Examples
    ///
    /// ```
    /// let product_line_version: Optional<&str> = Some("2017");
    /// let msbuild: MsBuild = MsBuild::find_msbuild(product_line_version);
    /// ```
    pub fn find_msbuild(product_line_version: Option<&str>) -> std::io::Result<Self> {
        product_line_version
            .map(ProductLineVersion::try_from)
            .transpose()
            .and_then(|potential_plv| {
                let max = potential_plv
                    .as_ref()
                    .map(|plv| plv.installation_version_max());
                let min = potential_plv
                    .as_ref()
                    .map(|plv| plv.installation_version_min());
                MsBuild::find_msbuild_in_range(max, min)
            })
    }

    /// Finds a msbuild that with the highest installation version that is in a range
    /// between max (exclusive) and min(inclusive).
    ///
    /// # Examples
    ///
    /// ```
    /// // Find the latest supported version for msbuild
    /// use msbuild::{MsBuild, ProductLineVersion};
    ///
    /// let msbuild = MsBuild::find_msbuild_in_range(
    ///     Some(ProductLineVersion::Vs2022.installation_version_max()),
    ///     Some(ProductLineVersion::Vs2017.installation_version_min()),
    /// );
    /// ```
    pub fn find_msbuild_in_range(
        max: Option<InstallationVersion>,
        min: Option<InstallationVersion>,
    ) -> std::io::Result<Self> {
        VsWhere::find_vswhere()
            .and_then(|vswhere| vswhere.run(None))
            .and_then(|output| Self::parse_from_json(&output))
            .and_then(|v: Value| {
                Self::list_instances(&v)
                    .and_then(|instances| Self::find_match(instances, max.as_ref(), min.as_ref()))
            })
            .map(|p| MsBuild {
                path: p.as_path().join("MsBuild/Current/Bin/msbuild.exe"),
            })
    }

    /// Executes msbuild using the provided project_path and
    /// the provided arguments.
    pub fn run(&self, project_path: &Path, args: &[&str]) -> std::io::Result<()> {
        if !self.path.as_path().exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Could not find [{}].", self.path.to_string_lossy()),
            ));
        }
        std::process::Command::new(self.path.as_path())
            .current_dir(project_path)
            .args(args)
            .output()
            .and_then(|out| {
                if out.status.success() {
                    Ok(())
                } else {
                    use std::io::Write;
                    std::io::stdout().write_all(&out.stdout)?;
                    let error_message = if let Some(code) = out.status.code() {
                        &format!("Failed to run msbuild: Exit code [{code}]")
                    } else {
                        "Failed to run msbuild"
                    };
                    Err(Error::other(format!(
                        "Failed to run msbuild: {error_message}"
                    )))
                }
            })
    }

    // Internal function for parsing a string as json object.
    fn parse_from_json(value: &str) -> std::io::Result<Value> {
        serde_json::from_str(value).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse command output as json ({})", e),
            )
        })
    }

    // Internal function for listing the instances inthe json value.
    fn list_instances(v: &Value) -> std::io::Result<&Vec<Value>> {
        v.as_array().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "json data did not contain any installation instances.",
            )
        })
    }

    // Internal function for finding the instances that matches the
    // version range and, if specified, the path in the environment
    // variable.
    fn find_match(
        instances_json: &[Value],
        max: Option<&InstallationVersion>,
        min: Option<&InstallationVersion>,
    ) -> std::io::Result<PathBuf> {
        let env_installation_path: Option<PathBuf> = std::env::var(MsBuild::ENV_KEY)
            .ok()
            .map(|v| PathBuf::from(&v));

        // Parse the instance json data and filter result based on version.
        let validated_instances = MsBuild::validate_instances_json(instances_json, max, min);

        if let Some(specified_installation_path) = env_installation_path {
            // Finds the specified installation path among the parsed
            // and validated instances.
            validated_instances
                .iter()
                .filter_map(|(_, p)| {
                    if specified_installation_path.starts_with(p) {
                        Some(p.to_path_buf())
                    } else {
                        None
                    }
                })
                .next()
                .ok_or(Error::new(
                    ErrorKind::NotFound,
                    "No instance found that matched requirements.",
                ))
        } else {
            // Select the latest version.
            validated_instances
                .iter()
                .max_by_key(|(v, _)| v)
                .map(|(_, p)| p.to_path_buf())
                .ok_or(Error::new(
                    ErrorKind::NotFound,
                    "No instance found that matched requirements.",
                ))
        }
    }

    /// Internal function that extracts a collection of parsed
    /// installation instances with a version within the given
    /// interval.
    fn validate_instances_json<'a>(
        instances_json: &'a [Value],
        max: Option<&'a InstallationVersion>,
        min: Option<&'a InstallationVersion>,
    ) -> Vec<(InstallationVersion<'a>, &'a Path)> {
        instances_json
            .iter()
            .filter_map(|i| {
                MsBuild::parse_installation_version(i)
                    .and_then(|installation_version| {
                        if MsBuild::has_version_in_range(
                            &installation_version.0,
                            max.map(|v| &v.0),
                            min.map(|v| &v.0),
                        ) {
                            MsBuild::parse_installation_path(i).map(|installation_path| {
                                Some((installation_version, installation_path))
                            })
                        } else {
                            // Maybe log(trace) that an instance was found that was not in the range.
                            Ok(None)
                        }
                    })
                    .unwrap_or_else(|e| {
                        print!("Encounted an error during parsing of instance data: {}", e);
                        None
                    })
            })
            .collect()
    }

    fn parse_installation_path(json_value: &Value) -> std::io::Result<&Path> {
        json_value
            .get("installationPath")
            .and_then(|path_json_value: &Value| path_json_value.as_str())
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                "Failed to retrieve `installationPath`.",
            ))
            .map(Path::new)
    }

    fn parse_installation_version(json_value: &Value) -> std::io::Result<InstallationVersion<'_>> {
        json_value
            .get("installationVersion")
            .and_then(|version_json_value: &Value| version_json_value.as_str())
            .and_then(|version_str: &str| Version::parse(version_str).ok())
            .map(InstallationVersion)
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                "Failed to retrieve `installationVersion`.",
            ))
    }

    /// Internal function to check if a version is in the range
    /// if it has been specified.
    fn has_version_in_range(
        version: &Version,
        max: Option<&Version>,
        min: Option<&Version>,
    ) -> bool {
        let is_below_max: bool = max.is_none_or(|max_version| max_version > version);
        let is_above_min: bool = min.is_none_or(|min_version| version >= min_version);
        is_below_max && is_above_min
    }
}

// ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Unit tests of the private functions and methods
// ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_msbuild_has_version_in_range() {
        let max = Some(
            Version::parse("4.3.2.1")
                .expect("It should be possible to create a Version object from the string 4.3.2.1"),
        );
        let min = Some(
            Version::parse("1.2.3.4")
                .expect("It should be possible to create a Version object from the string 1.2.3.4"),
        );
        // Check with no min or max
        assert!(
            MsBuild::has_version_in_range(
                &Version::parse("0.0.0.0").expect(
                    "It should be possible to create a Version object from the string 0.0.0.0"
                ),
                None,
                None
            ),
            "The version 0.0.0.0 should be in range when no min or max values have been specified."
        );
        // Check outside of range with min value.
        assert!(
            !MsBuild::has_version_in_range(
                &Version::parse("0.0.0.0").expect(
                    "It should be possible to create a Version object from the string 0.0.0.0"
                ),
                None,
                min.as_ref()
            ),
            "The version 0.0.0.0 should not be in range when min is 1.2.3.4"
        );
        // Check inside of range with min value
        assert!(
            MsBuild::has_version_in_range(
                &Version::parse("1.2.3.300").expect(
                    "It should be possible to create a Version object from the string 1.2.3.300"
                ),
                None,
                min.as_ref()
            ),
            "The version 1.2.3.300 should be in range when min is 1.2.3.4 and no max is given."
        );
        // Check out of range with max value
        assert!(
            !MsBuild::has_version_in_range(
                &Version::parse("4.3.2.11").expect(
                    "It should be possible to create a Version object from the string 4.3.2.11"
                ),
                max.as_ref(),
                None,
            ),
            "The version 4.3.2.11 should not be in range when max is 4.3.2.1 and no min is given."
        );
        // Check in range with max value
        assert!(
            MsBuild::has_version_in_range(
                &Version::parse("4.0.2.11").expect(
                    "It should be possible to create a Version object from the string 4.0.2.11"
                ),
                max.as_ref(),
                None,
            ),
            "The version 4.3.2.11 should not be in range when max is 4.3.2.1 and no min is given."
        );
        // Check in range with min and max
        assert!(
            MsBuild::has_version_in_range(
                &Version::parse("4.0.2.11").expect(
                    "It should be possible to create a Version object from the string 4.0.2.11"
                ),
                max.as_ref(),
                min.as_ref(),
            ),
            "The version 4.3.2.11 should not be in range when max is 4.3.2.1 and no max is given."
        );
    }

    #[test]
    fn test_msbuild_parse_installation_version() {
        let version_str = "2.3.1.34";
        let json_value = serde_json::json!({
            "instanceId": "VisualStudio.14.0",
            "installationPath": "C:\\Program Files (x86)\\Microsoft Visual Studio 14.0\\",
            "installationVersion": version_str
        });
        let expected = Version::parse(version_str)
            .map(InstallationVersion)
            .expect("It should be possible to parse the `version_str` as Version object.");
        let actual = MsBuild::parse_installation_version(&json_value).expect(
            "The function should be to extract an installation version from the json_value.",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_msbuild_parse_installation_path() {
        let expected = Path::new("C:\\Program Files (x86)\\Microsoft Visual Studio 14.0\\");
        let json_value = serde_json::json!({
            "instanceId": "019109ba",
            "installDate": "2023-08-26T14:05:02Z",
            "installationName": "VisualStudio/17.12.0+35506.116",
            "installationPath": expected.to_string_lossy(),
            "installationVersion": "17.12.35506.116",
            "productId": "Microsoft.VisualStudio.Product.Community",
            "productPath": "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\Common7\\IDE\\devenv.exe",
        });
        let actual = MsBuild::parse_installation_path(&json_value)
            .expect("The function should be to extract an installation path from the json_value.");
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_msbuild_validate_instances_json() {
        let json_value = serde_json::json!([
            {
                "installationPath": "C:\\Program Files (x86)\\Microsoft Visual Studio 14.0\\",
                "installationVersion": "14.0",
            },
            {
                "installationPath": "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community",
                "installationVersion": "17.12.35506.116",
            },
            {
                "installationPath": "C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise",
                "installationVersion": "17.08.35506.116",
            },
        ]);

        let values: &Vec<Value> = json_value
            .as_array()
            .expect("It should be possible to parse the json as an array of objects.");

        // Sanity check.
        assert_eq!(
            values.len(),
            3,
            "There should be 3 instances: \n {:?}",
            values
        );

        let min = Some(
            Version::parse("17.9")
                .map(InstallationVersion)
                .expect("It should be possible to parse the 17.9 as a version."),
        );
        let max = Some(
            Version::parse("18.0")
                .map(InstallationVersion)
                .expect("It should be possible to parse the 18.0 as a version."),
        );
        let validated_instances =
            MsBuild::validate_instances_json(values.as_slice(), max.as_ref(), min.as_ref());
        let expected_version = Version::parse("17.12.35506.116")
            .map(InstallationVersion)
            .expect("It should be possible to parse avlid version.");
        let expected_path =
            Path::new("C:\\Program Files\\Microsoft Visual Studio\\2022\\Community");
        assert_eq!(
            validated_instances.len(),
            1,
            "There should only be 1 element found."
        );
        let (actual_version, actual_path) = validated_instances.first().unwrap();
        assert_eq!(
            expected_version, *actual_version,
            "The returned version was not the expected one",
        );
        assert_eq!(
            expected_path, *actual_path,
            "The returned path was not the expected one."
        );
    }

    #[test]
    fn test_msbuild_find_match() {
        let json_value = serde_json::json!([
            {
                "installationPath": "C:\\Program Files (x86)\\Microsoft Visual Studio 14.0\\",
                "installationVersion": "14.0",
            },
            {
                "installationPath": "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community",
                "installationVersion": "17.12.35506.116",
            },
            {
                "installationPath": "C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise",
                "installationVersion": "17.08.35506.116",
            },
        ]);

        let values: &Vec<Value> = json_value
            .as_array()
            .expect("It should be possible to parse the json as an array of objects.");

        // Sanity check.
        assert_eq!(
            values.len(),
            3,
            "There should be 3 instances: \n {:?}",
            values
        );

        // The min and max are now chosen so that they will include
        // two possible result.
        let min = Some(
            Version::parse("17.7")
                .map(InstallationVersion)
                .expect("It should be possible to parse the 17.9 as a version."),
        );
        let max = Some(
            Version::parse("18.0")
                .map(InstallationVersion)
                .expect("It should be possible to parse the 18.0 as a version."),
        );

        // The expected values, when no environment variable have been set,
        // is the one with the latest version.
        let expected = PathBuf::from("C:\\Program Files\\Microsoft Visual Studio\\2022\\Community");

        let actual = MsBuild::find_match(values, max.as_ref(), min.as_ref())
            .expect("The function is expected to return a valid result.");

        assert_eq!(
            expected, actual,
            "The resulting path does not match the expected one."
        );
    }
}
