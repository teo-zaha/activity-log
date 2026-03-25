pub mod git;
pub mod gitlab;
pub mod jira;
pub mod outlook;
pub mod system;

pub(crate) fn load_required_envs(label: &str, names: &[&str]) -> Option<Vec<String>> {
    let values = names
        .iter()
        .map(|name| std::env::var(name).ok())
        .collect::<Option<Vec<_>>>();

    if values.is_none() {
        log_skip(label, &format!("{} not set", join_with_or(names)));
    }

    values
}

pub(crate) fn log_skip(label: &str, reason: &str) {
    eprintln!("  [{label}] skipped - {reason}");
}

fn join_with_or(values: &[&str]) -> String {
    match values {
        [] => String::new(),
        [value] => (*value).to_string(),
        [first, second] => format!("{first} or {second}"),
        _ => {
            let mut joined = values[..values.len() - 1].join(", ");
            joined.push_str(" or ");
            joined.push_str(values[values.len() - 1]);
            joined
        }
    }
}
