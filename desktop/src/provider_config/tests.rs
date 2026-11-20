use activity_log_core::{
    model::{
        AddConnectionRequestDto, BackendError, BackendErrorKind, ConnectionId,
        ProviderConfigConnectionStateDto, ProviderConfigFieldValueDto,
        ProviderConfigSaveRequestDto, ProviderConfigSaveValueDto, ProviderConfigValueDto,
        ProviderConfigValueTypeDto, Source, SupportedOs,
    },
    provider::{connection::Connection, git::default_git_root},
    settings::ConfigFile,
};

use super::{apply_add, apply_save, build_snapshot, provider_config_key_label, save_changes};

const OS: SupportedOs = SupportedOs::Macos;

fn file(text: &str) -> ConfigFile {
    ConfigFile::parse("config.toml", text.as_bytes()).unwrap()
}

fn connections(file: &ConfigFile) -> Vec<Connection> {
    file.connections().connections
}

fn connection_state(file: &ConfigFile, id: &str) -> ProviderConfigConnectionStateDto {
    let id = id.parse::<ConnectionId>().unwrap();
    build_snapshot(&connections(file), &OS)
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

#[test]
fn label_formatter_keeps_common_acronyms_readable() {
    assert_eq!(provider_config_key_label("GITLAB_URL"), "GitLab URL");
    assert_eq!(provider_config_key_label("JIRA_INSECURE"), "Jira Insecure");
    assert_eq!(provider_config_key_label("GIT_DIRS"), "Git Dirs");
}

#[test]
fn snapshot_lists_kinds_and_connections() {
    let empty = build_snapshot(&connections(&file("")), &OS);
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
    let on_linux = build_snapshot(&[], &SupportedOs::Linux);
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

    let snapshot = build_snapshot(&connections(&file("[[git]]\n")), &OS);
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

    let debug = format!("{:?}", build_snapshot(&connections(&file), &OS));

    assert!(!debug.contains("query-secret"), "{debug}");
    assert!(!debug.contains("glpat-secret"), "{debug}");
}
