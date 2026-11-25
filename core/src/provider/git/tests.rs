use super::unreadable_root;
use crate::{
    model::ProviderProblem,
    provider::source_failure::{ClassifiedProblem, FixCause, problem_of},
};

#[test]
fn only_eperm_for_a_git_root_can_offer_files_and_folders() {
    for (code, classification) in [
        (
            libc::EPERM,
            ClassifiedProblem::caused(FixCause::FolderDenied),
        ),
        (
            libc::EACCES,
            ClassifiedProblem::plain(ProviderProblem::PermissionDenied),
        ),
    ] {
        let error = unreadable_root(
            std::path::Path::new("/example/widgets"),
            &std::io::Error::from_raw_os_error(code),
        );
        assert_eq!(problem_of(&error.into()), Some(classification));
    }
}
