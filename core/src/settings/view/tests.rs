use crate::{
    model::{
        AddConnectionRequestDto, BackendError, BackendErrorKind, ConnectionId,
        ProviderConfigConnectionStateDto, ProviderConfigFieldRefDto, ProviderConfigFieldValueDto,
        ProviderConfigIssueKindDto, ProviderConfigSaveRequestDto, ProviderConfigSaveValueDto,
        ProviderConfigValueDto, ProviderConfigValueTypeDto, Source, SupportedOs,
    },
    provider::{connection::Connection, git::default_git_root},
    settings::ConfigFile,
};

use super::{
    SaveBase, Submitted, apply_add, apply_save, build_snapshot, check_save, normalize, save_changes,
};

const OS: SupportedOs = SupportedOs::Macos;

fn file(text: &str) -> ConfigFile {
    ConfigFile::parse("config.toml", text.as_bytes()).unwrap()
}

fn connections(file: &ConfigFile) -> Vec<Connection> {
    file.connections().connections
}

fn connection_state(file: &ConfigFile, id: &str) -> ProviderConfigConnectionStateDto {
    let id = id.parse::<ConnectionId>().unwrap();
    build_snapshot(&connections(file), &OS, "r1".to_string())
        .connections
        .into_iter()
        .find(|state| state.connection == id)
        .unwrap_or_else(|| panic!("no connection {id}"))
}

fn field(state: &ProviderConfigConnectionStateDto, key: &str) -> ProviderConfigFieldValueDto {
    state
        .fields
        .iter()
        .find(|field| field.key == key)
        .unwrap_or_else(|| panic!("no field {key}"))
        .value
        .clone()
}

fn save(
    connection: &str,
    values: Vec<(&str, Option<ProviderConfigValueDto>)>,
) -> ProviderConfigSaveRequestDto {
    ProviderConfigSaveRequestDto {
        connection: connection.parse().unwrap(),
        label: None,
        values: values
            .into_iter()
            .map(|(key, value)| ProviderConfigSaveValueDto {
                key: key.to_string(),
                value,
            })
            .collect(),
        based_on: None,
    }
}

/// Checks `request` and applies it, as the save command does under the config lock.
fn apply(
    file: &mut ConfigFile,
    request: &ProviderConfigSaveRequestDto,
) -> Result<(), BackendError> {
    let changes = save_changes(request)?;
    Ok(apply_save(file, &changes)?)
}

fn text(value: &str) -> Option<ProviderConfigValueDto> {
    Some(ProviderConfigValueDto::Text(value.to_string()))
}

/// Each key is labelled where it is declared; the labels read as they did when they were
/// built from the key's name.
#[test]
fn fields_are_labelled_as_their_keys_declare() {
    let file = file(
        "[[gitlab]]\nlabel = \"Work\"\n\n[[jira]]\nlabel = \"Jira\"\n\n[[git]]\nlabel = \"Git\"\n",
    );
    let labels = build_snapshot(&connections(&file), &OS, "r1".to_string())
        .connections
        .into_iter()
        .flat_map(|state| state.fields)
        .map(|field| (field.key, field.label))
        .collect::<Vec<_>>();
    let label = |key: &str| {
        labels
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, label)| label.as_str())
    };
    assert_eq!(label("GITLAB_URL"), Some("GitLab URL"));
    assert_eq!(label("JIRA_INSECURE"), Some("Jira Insecure"));
    assert_eq!(label("GIT_DIRS"), Some("Git Dirs"));
}

#[test]
fn snapshot_lists_kinds_and_connections() {
    let empty = build_snapshot(&connections(&file("")), &OS, "r1".to_string());
    assert!(empty.connections.is_empty());
    let gitlab = empty
        .kinds
        .iter()
        .find(|kind| kind.source == Source::Gitlab)
        .unwrap();
    assert!(gitlab.multiple && gitlab.supported);
    assert_eq!(
        gitlab.description,
        "Merge requests, pushes, comments and reviews"
    );
    assert!(empty.kinds.iter().all(|kind| !kind.description.is_empty()));
    let on_linux = build_snapshot(&[], &SupportedOs::Linux, "r1".to_string());
    let outlook = on_linux
        .kinds
        .iter()
        .find(|kind| kind.source == Source::Outlook)
        .unwrap();
    assert!(!outlook.multiple);
    assert!(!outlook.supported);
    assert_eq!(
        outlook.supported_os,
        [SupportedOs::Windows, SupportedOs::Macos]
    );

    let snapshot = build_snapshot(&connections(&file("[[git]]\n")), &OS, "r1".to_string());
    assert_eq!(
        snapshot.connections[0].connection,
        ConnectionId::new(Source::Git, "this-computer")
    );
    assert_eq!(snapshot.connections[0].label, "This computer");
}

#[test]
fn snapshot_masks_secrets_and_shows_the_default_git_folder() {
    let config = file("[[gitlab]]\nlabel = \"Work\"\ntoken = \"secret\"\n\n[[git]]\n");

    let gitlab = connection_state(&config, "gitlab/work");
    let token = gitlab
        .fields
        .iter()
        .find(|field| field.key == "GITLAB_TOKEN")
        .unwrap();
    assert!(token.is_set);
    assert_eq!(
        token.value,
        ProviderConfigFieldValueDto::Single {
            value_type: ProviderConfigValueTypeDto::Password,
            secret: true,
            value: None,
        }
    );
    let default_root = default_git_root().expect("home directory");
    assert_eq!(
        field(&connection_state(&config, "git/this-computer"), "GIT_DIRS"),
        ProviderConfigFieldValueDto::List {
            item_type: ProviderConfigValueTypeDto::String,
            items: vec![default_root.display().to_string()],
        }
    );
}

#[test]
fn saves_values_and_labels_of_one_connection() {
    let mut config = file("[[gitlab]]\n");
    let mut request = save(
        "gitlab/gitlab",
        vec![
            ("GITLAB_URL", text(" https://gitlab.example.com ")),
            ("GITLAB_USER", text("")),
        ],
    );
    request.label = Some("Work".to_string());
    apply(&mut config, &request).unwrap();

    let gitlab = connection_state(&config, "gitlab/gitlab");
    assert_eq!(gitlab.label, "Work");
    assert_eq!(
        field(&gitlab, "GITLAB_URL"),
        ProviderConfigFieldValueDto::Single {
            value_type: ProviderConfigValueTypeDto::Url,
            secret: false,
            value: Some("https://gitlab.example.com".to_string()),
        }
    );
    let refused = |request| apply(&mut file("[[gitlab]]\n"), &request).unwrap_err().kind;
    assert_eq!(
        refused(save("gitlab/gitlab", vec![("GITLAB_URL", text("nope"))])),
        BackendErrorKind::InvalidRequest
    );
    assert_eq!(
        refused(save("gitlab/gitlab", vec![("JIRA_URL", text("x"))])),
        BackendErrorKind::InvalidRequest
    );
    // Another program removed it meanwhile.
    assert_eq!(
        refused(save(
            "gitlab/gone",
            vec![("GITLAB_URL", text("https://a.example.org"))]
        )),
        BackendErrorKind::NotFound
    );
    // Also for a save that changes nothing.
    assert_eq!(
        refused(save("gitlab/gone", vec![])),
        BackendErrorKind::NotFound
    );
}

#[test]
fn saves_lists_cleaned_and_keeps_an_empty_list_apart_from_a_cleared_one() {
    let mut config = file("[[git]]\n");
    let items = vec![
        " ~/work ".to_string(),
        String::new(),
        "\"/srv/repos\"".to_string(),
    ];
    apply(
        &mut config,
        &save(
            "git/this-computer",
            vec![("GIT_DIRS", Some(ProviderConfigValueDto::List(items)))],
        ),
    )
    .unwrap();
    assert_eq!(
        field(&connection_state(&config, "git/this-computer"), "GIT_DIRS"),
        ProviderConfigFieldValueDto::List {
            item_type: ProviderConfigValueTypeDto::String,
            items: vec!["~/work".to_string(), "/srv/repos".to_string()],
        }
    );

    apply(
        &mut config,
        &save(
            "git/this-computer",
            vec![("GIT_DIRS", Some(ProviderConfigValueDto::List(Vec::new())))],
        ),
    )
    .unwrap();
    let git = connection_state(&config, "git/this-computer");
    assert!(git.fields[0].is_set);
    assert!(config.to_string().contains("dirs = []"));

    apply(
        &mut config,
        &save("git/this-computer", vec![("GIT_DIRS", None)]),
    )
    .unwrap();
    assert!(!connection_state(&config, "git/this-computer").fields[0].is_set);
}

#[test]
fn rejects_a_list_for_a_single_value_key() {
    let mut config = file("[[jira]]\n");
    let request = save(
        "jira/jira",
        vec![(
            "JIRA_URL",
            Some(ProviderConfigValueDto::List(vec!["https://a".into()])),
        )],
    );

    assert_eq!(
        apply(&mut config, &request).unwrap_err().kind,
        BackendErrorKind::InvalidRequest
    );
}

#[test]
fn adds_wsl_git_connections_with_home_as_the_default_folder() {
    let mut config = file("");
    let id = apply_add(
        &mut config,
        &AddConnectionRequestDto {
            source: Source::Git,
            label: None,
            wsl_distro: Some("Ubuntu".to_string()),
        },
    )
    .unwrap();

    let state = connection_state(&config, &id.to_string());
    assert_eq!(state.label, "WSL · Ubuntu");
    assert_eq!(state.wsl_distro.as_deref(), Some("Ubuntu"));
    assert_eq!(
        field(&state, "GIT_DIRS"),
        ProviderConfigFieldValueDto::List {
            item_type: ProviderConfigValueTypeDto::String,
            items: vec!["~".to_string()],
        }
    );
}

#[test]
fn a_second_connection_of_a_single_kind_already_exists() {
    let mut config = file("[[outlook]]\n");
    let refused = apply_add(
        &mut config,
        &AddConnectionRequestDto {
            source: Source::Outlook,
            label: None,
            wsl_distro: None,
        },
    )
    .unwrap_err();

    assert_eq!(
        BackendError::from(refused).kind,
        BackendErrorKind::AlreadyExists
    );
}

#[test]
fn debug_output_of_a_settings_snapshot_shows_no_url_query() {
    let file = file(
        "[[gitlab]]\nname = \"work\"\nurl = \"https://gitlab.example.com/?private_token=query-secret\"\ntoken = \"glpat-secret\"\nuser = \"jane\"\n",
    );

    let debug = format!(
        "{:?}",
        build_snapshot(&connections(&file), &OS, "r1".to_string())
    );

    assert!(!debug.contains("query-secret"), "{debug}");
    assert!(!debug.contains("glpat-secret"), "{debug}");
}

fn key_of(source: Source, name: &str) -> crate::provider::provider_config::ProviderConfigKey {
    crate::provider::provider_registry()
        .into_iter()
        .find(|adapter| adapter.id() == source)
        .and_then(|adapter| {
            adapter
                .config_requirements()
                .iter()
                .copied()
                .find(|key| key.name == name)
        })
        .unwrap_or_else(|| panic!("no key {name}"))
}

fn problem(source: Source, name: &str, value: &str) -> Option<String> {
    normalize(key_of(source, name), Submitted::Text(value))
        .err()
        .map(|issues| issues[0].message.clone())
}

/// One rule per value type (B64: Rust accepted any URL scheme, the pane only http and https).
#[test]
fn values_are_checked_by_their_type() {
    let url = |value| problem(Source::Jira, "JIRA_URL", value);
    assert_eq!(url("https://jira.example.org/jira"), None);
    assert_eq!(url("http://jira.example.org"), None);
    for bad in [
        "ftp://x",
        "https://",
        "jira.example.org",
        "mailto:a@example.org",
    ] {
        assert_eq!(
            url(bad).as_deref(),
            Some("Enter an http or https address."),
            "{bad}"
        );
    }
    assert_eq!(
        url("https://user:pass@jira.example.org").as_deref(),
        Some("Enter the address without a user name or password.")
    );
    let insecure = |value| problem(Source::Jira, "JIRA_INSECURE", value);
    assert_eq!(insecure(" TRUE "), None);
    assert_eq!(insecure("yes").as_deref(), Some("Enter true or false."));
    // A token is any text, and a blank one clears the key.
    assert_eq!(problem(Source::Jira, "JIRA_TOKEN", "a b c"), None);
    assert_eq!(
        normalize(key_of(Source::Jira, "JIRA_TOKEN"), Submitted::Text("  ")),
        Ok(None)
    );
}

#[test]
fn the_rule_the_file_is_read_with_refuses_other_schemes_too() {
    use crate::provider::provider_config::{
        ProviderConfigValueType, provider_config_value_matches_type,
    };
    assert!(!provider_config_value_matches_type(
        "ftp://x",
        ProviderConfigValueType::Url
    ));
    // A file that holds user info still loads: its check says what to change.
    assert!(provider_config_value_matches_type(
        "https://user:pass@jira.example.org",
        ProviderConfigValueType::Url
    ));
}

/// The CLI's several arguments are a list, one argument its text form: both store the same.
#[test]
fn a_list_as_items_or_as_text_is_stored_alike() {
    let dirs = key_of(Source::Git, "GIT_DIRS");
    let items = ["~/work".to_string(), "/srv/repos".to_string()];
    assert_eq!(
        normalize(dirs, Submitted::List(&items)),
        normalize(dirs, Submitted::Text("~/work, /srv/repos"))
    );
    assert_eq!(
        normalize(dirs, Submitted::List(&[])),
        Ok(Some("[]".to_string()))
    );
}

#[test]
fn a_save_lists_every_issue_in_request_order_without_the_values() {
    let mut request = save(
        "gitlab/work",
        vec![
            ("GITLAB_URL", text("ftp://gitlab.example.org")),
            ("GITLAB_SHOE", text("x")),
            (
                "GITLAB_USER",
                Some(ProviderConfigValueDto::List(vec!["a".into()])),
            ),
        ],
    );
    request.label = Some("  ".to_string());

    let issues = check_save(&request, SaveBase::None, None);

    assert_eq!(
        issues
            .iter()
            .map(|issue| (issue.field.clone(), issue.message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                ProviderConfigFieldRefDto::Label,
                "A connection needs a label."
            ),
            (
                ProviderConfigFieldRefDto::Value {
                    key: "GITLAB_URL".into()
                },
                "Enter an http or https address."
            ),
            (
                ProviderConfigFieldRefDto::Value {
                    key: "GITLAB_SHOE".into()
                },
                "This kind has no such setting."
            ),
            (
                ProviderConfigFieldRefDto::Value {
                    key: "GITLAB_USER".into()
                },
                "Enter one value, not a list."
            ),
        ]
    );
    assert!(issues.iter().all(|issue| !issue.message.contains("ftp")));
}

/// An item's issue names its place among the items sent, blank rows not counted.
#[test]
fn an_item_issue_names_its_place() {
    let mut key = key_of(Source::Git, "GIT_DIRS");
    key.shape = crate::provider::provider_config::ProviderConfigShape::List(
        crate::provider::provider_config::ProviderConfigValueType::Url,
    );
    let items = [
        "https://a.example.org".to_string(),
        String::new(),
        "nope".to_string(),
    ];
    let issues = normalize(key, Submitted::List(&items)).unwrap_err();
    assert_eq!(
        issues[0].field,
        ProviderConfigFieldRefDto::Item {
            key: "GIT_DIRS".into(),
            index: 1
        }
    );
}

/// A refused save names the field as the settings show it, then the issue.
#[test]
fn a_refused_save_says_its_first_issue() {
    let refused = save_changes(&save(
        "jira/jira",
        vec![("JIRA_URL", text("ftp://jira.example.org"))],
    ))
    .err()
    .unwrap();
    assert_eq!(refused.kind, BackendErrorKind::InvalidRequest);
    assert_eq!(refused.message, "Jira URL: Enter an http or https address.");
}

/// What another program changed since the draft's base is listed as issues of their own kind,
/// one per field, never with a value; a connection that is gone is one issue for the whole.
#[test]
fn a_check_against_the_current_file_names_what_changed_since_the_base() {
    use crate::settings::FieldId;
    let read = file("[[gitlab]]\nlabel = \"Work\"\nurl = \"https://gitlab.example.org\"\n");
    let id = "gitlab/work".parse::<ConnectionId>().unwrap();
    let mut request = save("gitlab/work", vec![]);
    request.label = Some("Mine".to_string());
    let base = || SaveBase::Known(read.edit_base(&id, &[FieldId::Label]).unwrap());

    let unchanged = check_save(&request, base(), Some(&read));
    assert!(unchanged.is_empty(), "{unchanged:?}");

    let relabelled = file(
        "[[gitlab]]\nlabel = \"Theirs\"\nname = \"work\"\nurl = \"https://gitlab.example.org\"\n",
    );
    let issues = check_save(&request, base(), Some(&relabelled));
    assert_eq!(
        issues
            .iter()
            .map(|issue| (issue.field.clone(), issue.problem))
            .collect::<Vec<_>>(),
        [(
            ProviderConfigFieldRefDto::Label,
            ProviderConfigIssueKindDto::Changed
        )]
    );
    assert!(issues.iter().all(|issue| !issue.message.contains("Theirs")));

    // Another account: an untouched account field changed too.
    let moved = file("[[gitlab]]\nlabel = \"Work\"\nurl = \"https://other.example.org\"\n");
    assert_eq!(
        check_save(&request, base(), Some(&moved))[0].field,
        ProviderConfigFieldRefDto::Value {
            key: "GITLAB_URL".into()
        }
    );

    let gone = file("");
    assert_eq!(
        check_save(&request, base(), Some(&gone))[0].problem,
        ProviderConfigIssueKindDto::Removed
    );
    assert_eq!(
        check_save(&request, SaveBase::Unknown(id), Some(&read))[0].problem,
        ProviderConfigIssueKindDto::UnknownBase
    );
}
