use super::{Chosen, Profile, UnknownProfile, choose};

#[test]
fn unset_or_empty_is_the_installed_profile() {
    for debug_build in [true, false] {
        for value in [None, Some("")] {
            assert_eq!(
                choose(value, debug_build),
                Ok(Chosen::Profile(Profile::Installed))
            );
        }
    }
}

/// A debug build takes `dev` and refuses anything else, case included.
#[test]
fn a_debug_build_takes_only_dev() {
    assert_eq!(choose(Some("dev"), true), Ok(Chosen::Profile(Profile::Dev)));
    for value in ["Dev", "prod", "installed", " dev"] {
        assert_eq!(
            choose(Some(value), true),
            Err(UnknownProfile(value.to_string()))
        );
    }
}

/// A release build never stops for the variable, whatever it holds.
#[test]
fn a_release_build_ignores_any_value() {
    for value in ["dev", "prod", "anything"] {
        assert_eq!(choose(Some(value), false), Ok(Chosen::IgnoredInRelease));
    }
}

#[test]
fn each_profile_has_its_own_folder() {
    assert_eq!(Profile::Installed.folder(), "activity-log");
    assert_eq!(Profile::Dev.folder(), "activity-log-dev");
}
