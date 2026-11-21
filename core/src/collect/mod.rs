pub mod calendar;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        unused_imports,
        reason = "the GitLab lookup and `annotate` use it (plan step 16, stages 5 and 6)"
    )
)]
pub(crate) mod change_stack;
pub mod git;
pub mod gitlab;
pub mod jira;
mod meeting_text;
pub mod outlook;
pub mod system;
