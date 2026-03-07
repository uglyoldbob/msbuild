use msbuild::VsWhere;

#[cfg_attr(not(feature = "has-vs2022"), ignore)]
#[test]
fn test_find_vswhere() {
    // Cannot run the tests unless vswhere has
    // been installed into the test environment.

    assert!(VsWhere::find_vswhere().is_ok());
}

#[ignore]
#[test]
fn test_find_vswhere_env() {
    // Cannot run this unless there is a vswhere
    // installed in a non standard location in the
    // test environment.

    // This function is only safe to call on windows
    // in a single threaded context. Make sure that the
    // test environment has a vswhere.exe in the specified
    // location.
    const NON_STANDRAD_VSWHERE_LOCATION: &str =
        "C:\\Program Files (x86)\\Other\\Installer\\vswhere.exe";
    unsafe {
        std::env::set_var("VS_WHERE_PATH", NON_STANDRAD_VSWHERE_LOCATION);
    }

    assert!(VsWhere::find_vswhere().is_ok());
}

#[cfg_attr(not(feature = "has-vs2022"), ignore)]
#[test]
fn test_run() {
    // Cannot run the tests unless vswhere has
    // been installed into the test environment.

    let vs_where: VsWhere =
        VsWhere::find_vswhere().expect("vswhere should have been found if it was installed.");

    let args: [&str; 4] = ["-format", "json", "-products", "*"];
    let result = vs_where
        .run(Some(args.as_slice()))
        .expect("Calling with vswhere with valid args should not return an error.");

    assert!(
        !result.is_empty(),
        "The returned string from calling vswhere was empty."
    );
}
