//! Activity Log 0.0.7's sources, imported once (`docs/adr/0006-secure-secrets.md`): 0.0.7
//! had no `config.toml`, so a user upgrading from it would start with none. The first start of
//! the desktop app or of a CLI command that reads the sources imports them, only on the
//! default config path, only while no `config.toml` and no marker exist there, and only from a
//! legacy file found. The legacy files are only read.
//!
//! The order: read and map the legacy file; if it brings a token, read the key (created over a
//! definite absence); then, under the config lock with `Precondition::FileAbsent`, add the
//! connections, seal, check that each sealed value opens, publish, and write the marker.

use std::path::{Path, PathBuf};

use self::{
    files::{EnvFound, LegacyFile, Read, Unreadable},
    marker::{MARKER, Marker, Record, given_up_toml, marker_beside},
    plan::Plan,
    v0_0_7::LegacyValues,
};
use super::{
    ConfigFile, EditError, Precondition,
    write::{edit_config_recorded, record_beside},
};
use crate::{
    diagnostics,
    model::{
        ConnectionId, LegacyFileDto, LegacyImportReportDto, LegacyImportSkipDto, LegacySkipReason,
        LegacySource, LegacyStoreWait, SupportedOs,
    },
    profile::{self, Profile},
    secrets::{FrontEnd, Interaction, Secrets, StoreErrorKind},
};

mod files;
mod marker;
mod plan;
#[cfg(test)]
mod tests;
mod v0_0_7;

const SCOPE: &str = "import";

/// Where one front end imports to and looks in. Only [`Place::for_front_end`] makes one in
/// production, and only on the default config path.
#[derive(Debug, Clone)]
pub struct Place {
    config: PathBuf,
    /// `settings.json`, the desktop's and the CLI's.
    settings: Option<PathBuf>,
    /// Where the walk for `.env` starts: the CLI's and the API's working directory.
    env_from: Option<PathBuf>,
    os: SupportedOs,
    wsl_distros: fn() -> Vec<String>,
}

impl Place {
    /// The place for `front_end`, or `None` where no import runs: a config path given
    /// (`--config`, `explicit`, or `ACTIVITY_LOG_CONFIG`), the dev profile, or no settings
    /// location.
    pub fn for_front_end(front_end: FrontEnd, explicit: Option<&Path>) -> Option<Self> {
        let chosen =
            explicit.is_some() || crate::platform::dirs::env_path("ACTIVITY_LOG_CONFIG").is_some();
        if !runs_with(chosen, profile::current()) {
            return None;
        }
        let config = ConfigFile::resolve_path(None)?;
        let env_from = match front_end {
            FrontEnd::Desktop => None,
            FrontEnd::Cli | FrontEnd::Api => std::env::current_dir().ok(),
        };
        Some(Self {
            config,
            settings: files::settings_json_path(),
            env_from,
            os: crate::provider::current_os(),
            wsl_distros: crate::provider::git::default_wsl_distros,
        })
    }

    /// Whether an import would run here: no `config.toml` (an empty one counts), no marker
    /// (one that does not read counts too, logged).
    fn is_due(&self) -> bool {
        if std::fs::symlink_metadata(&self.config).is_ok() {
            return false;
        }
        match marker_beside(&self.config) {
            Marker::Absent => true,
            Marker::Present => false,
            Marker::Unreadable => {
                diagnostics::warn(
                    SCOPE,
                    format!(
                        "{} is no record of an import Activity Log can read; 0.0.7's settings are not imported again",
                        self.config.with_file_name(MARKER).display()
                    ),
                );
                false
            }
        }
    }
}

/// Whether an import runs at all: only on the default config path (`chosen`: none given with
/// `--config` or `ACTIVITY_LOG_CONFIG`) and the installed profile.
fn runs_with(chosen: bool, profile: Profile) -> bool {
    !chosen && profile == Profile::Installed
}

/// The API server never imports: finding 0.0.7's settings and no `config.toml`, it says how
/// to import them.
pub fn hint(place: &Place) {
    if !place.is_due() {
        return;
    }
    let found = place
        .settings
        .as_ref()
        .filter(|path| path.is_file())
        .cloned()
        .or_else(|| match place.env_from.as_deref().map(files::find_env) {
            Some(EnvFound::Found { path, bytes }) => match files::read_env(&bytes) {
                Read::Values { values, .. } => values.sets_any().then_some(path),
                Read::Unreadable(_) => None,
            },
            _ => None,
        });
    if let Some(path) = found {
        diagnostics::info(
            SCOPE,
            format!(
                "Found Activity Log 0.0.7 settings in {}; run `activity-log sources` once to import them",
                path.display()
            ),
        );
    }
}

/// What [`give_up`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveUp {
    GaveUp,
    /// No import was due here: nothing was written.
    NotDue,
}

/// Gives the import up where it is due: the marker says so, and 0.0.7's settings are never
/// imported, so a command may write `config.toml` meanwhile.
pub fn give_up(place: &Place) -> Result<GiveUp, EditError> {
    if !place.is_due() {
        return Ok(GiveUp::NotDue);
    }
    let marker = given_up_toml(&crate::utils::now_iso());
    record_beside(&place.config, MARKER, marker.as_bytes())?;
    diagnostics::info(
        SCOPE,
        "The import of Activity Log 0.0.7's settings was given up",
    );
    Ok(GiveUp::GaveUp)
}

/// The one import, where it is due: what it did, also logged, or `None` when nothing was due or
/// found, or another program wrote the settings first. The key is read with `interaction`: the
/// desktop's start asks without a dialog, and with one only when the user asks for the import.
pub async fn import_once(
    place: &Place,
    secrets: &Secrets,
    interaction: Interaction,
) -> Option<LegacyImportReportDto> {
    if !place.is_due() {
        return None;
    }
    let chosen = choose(place)?;
    let (used, values) = match chosen.used {
        Chosen::Values(file, values) => (file, values),
        Chosen::Unreadable(file, why) => {
            let message =
                format!("Activity Log 0.0.7's {file} could not be imported: {why}. {GIVES_UP}");
            diagnostics::warn(SCOPE, &message);
            return Some(LegacyImportReportDto::Unreadable {
                file: dto(&file),
                message,
            });
        }
    };
    for key in values.unknown_keys() {
        diagnostics::info(
            SCOPE,
            format!("{key} in {used} is not a setting 0.0.7 read; left out"),
        );
    }
    let plan = plan::plan(&values, place.os.clone(), place.wsl_distros);
    let sealing = match plan.connections.iter().any(plan::Planned::has_secret) {
        false => None,
        // A new config holds no sealed value, so no key it could have been made with.
        true => match secrets.sealer(interaction, |_| 0).await {
            Ok(sealer) => Some(sealer),
            Err(err) => {
                let store = match err.kind() {
                    StoreErrorKind::Unavailable => LegacyStoreWait::Unavailable,
                    StoreErrorKind::Refused | StoreErrorKind::Failed => LegacyStoreWait::Refused,
                };
                let message = format!(
                    "Importing Activity Log 0.0.7's sources from {used} waits for the secure store, since they bring a token: {err}. {GIVES_UP}"
                );
                diagnostics::warn(SCOPE, &message);
                return Some(LegacyImportReportDto::Postponed {
                    file: dto(&used),
                    store,
                    message,
                });
            }
        },
    };

    let config = place.config.clone();
    let record = Record {
        at: crate::utils::now_iso(),
        used: used.source,
        path: used.path.display().to_string(),
        found_unused: chosen
            .unused
            .iter()
            .map(|file| file.path.display().to_string())
            .collect(),
        imported: Vec::new(),
        skipped: Vec::new(),
    };
    let invalid_wsl_repos = plan.invalid_wsl_repos;
    let withheld_wsl = plan.withheld_wsl.clone();
    let written = tokio::task::spawn_blocking(move || {
        edit_config_recorded(
            &config,
            Precondition::FileAbsent,
            sealing.as_ref(),
            move |file| add_planned(file, plan),
            MARKER,
            |(imported, skipped): &(Vec<ConnectionId>, Vec<LegacyImportSkipDto>)| {
                Record {
                    imported: imported.iter().map(ToString::to_string).collect(),
                    skipped: skipped.clone(),
                    ..record
                }
                .to_toml()
                .into_bytes()
            },
        )
    })
    .await;
    let (edited, recorded) = match written {
        Ok(Ok(written)) => written,
        Ok(Err(EditError::FileExists)) => {
            diagnostics::info(
                SCOPE,
                "Another program wrote the settings first; 0.0.7's settings were not imported",
            );
            return None;
        }
        Ok(Err(err)) => return Some(failed(&used, &err)),
        Err(err) => return Some(failed(&used, &err)),
    };
    if let Err(err) = recorded {
        diagnostics::warn(
            SCOPE,
            format!("The record of the import could not be written ({err}); the settings were"),
        );
    }
    for warning in &edited.warnings {
        diagnostics::warn(SCOPE, warning);
    }
    let (imported, mut skipped) = edited.value;
    skipped.extend((0..invalid_wsl_repos).map(|_| LegacyImportSkipDto {
        connection: None,
        field: v0_0_7::WSL_GIT_DIRS.to_string(),
        reason: LegacySkipReason::InvalidWslRepo,
    }));
    skipped.extend(
        withheld_wsl
            .into_iter()
            .map(|(field, reason)| LegacyImportSkipDto {
                connection: None,
                field: field.to_string(),
                reason,
            }),
    );
    for skip in &skipped {
        diagnostics::warn(SCOPE, skip_line(skip));
    }
    let mut message = format!(
        "Imported {} sources from Activity Log 0.0.7 ({used})",
        imported.len()
    );
    if let Some(unused) = &chosen.unused {
        message.push_str(&format!("; also found {unused}, not imported"));
    }
    if !skipped.is_empty() {
        message.push_str(&format!(
            "; {} of its values were left out, as the Logs say",
            skipped.len()
        ));
    }
    diagnostics::info(SCOPE, &message);
    Some(LegacyImportReportDto::Imported {
        file: dto(&used),
        unused: chosen.unused.as_ref().map(dto),
        imported,
        skipped,
        message,
    })
}

/// What a pending import's report adds: a `config.toml` written meanwhile ends it.
const GIVES_UP: &str = "Adding a source before then gives the import up.";

/// The import due and not done: its write failed (a busy lock, a location that keeps no file
/// private, a seal that did not check), and it is tried again at the next start.
fn failed(used: &LegacyFile, err: &dyn std::fmt::Display) -> LegacyImportReportDto {
    let message = format!(
        "Importing Activity Log 0.0.7's sources from {used} failed: {err}. It is tried again at the next start. {GIVES_UP}"
    );
    diagnostics::warn(SCOPE, &message);
    LegacyImportReportDto::Failed {
        file: dto(used),
        message,
    }
}

/// Which legacy file the import reads.
#[derive(Debug)]
enum Chosen {
    Values(LegacyFile, LegacyValues),
    Unreadable(LegacyFile, Unreadable),
}

#[derive(Debug)]
struct Choice {
    used: Chosen,
    unused: Option<LegacyFile>,
}

/// `settings.json` wins when it sets a key 0.0.7 read, by the import's own rules (Q12);
/// otherwise a `.env` that reads, then `settings.json` that sets nothing (0.0.7 still ran Git
/// and the local sources from it), then whichever could not be read, so the report says why.
fn choose(place: &Place) -> Option<Choice> {
    let settings = place.settings.as_ref().and_then(|path| {
        files::read_settings(path).map(|read| {
            let file = LegacyFile {
                source: LegacySource::Settings,
                path: path.clone(),
            };
            (file, read)
        })
    });
    let env_file = |path| LegacyFile {
        source: LegacySource::Env,
        path,
    };
    let env = match place.env_from.as_deref().map(files::find_env) {
        Some(EnvFound::Found { path, bytes }) => match files::read_env(&bytes) {
            // A project's own `.env` (a database address, say) is no settings file of 0.0.7's.
            Read::Values { values, .. } if !values.sets_any() => None,
            read => Some((env_file(path), read)),
        },
        Some(EnvFound::Unreadable(path, kind)) => {
            Some((env_file(path), Read::Unreadable(Unreadable::Io(kind))))
        }
        Some(EnvFound::NotOwned(path)) => {
            diagnostics::warn(
                SCOPE,
                format!(
                    "{} belongs to another account; it is not imported",
                    path.display()
                ),
            );
            None
        }
        Some(EnvFound::None) | None => None,
    };
    let values = |found: Option<(LegacyFile, Read)>| match found {
        Some((file, Read::Values { values, stopped })) => {
            if stopped {
                diagnostics::warn(
                    SCOPE,
                    format!(
                        "{file}: a line does not read; it and the lines after it are left out, as 0.0.7 left them"
                    ),
                );
            }
            Ok((file, values))
        }
        Some((file, Read::Unreadable(why))) => Err(Some((file, why))),
        None => Err(None),
    };
    let (settings, env) = (values(settings), values(env));
    let file_of = |found: &Result<(LegacyFile, LegacyValues), Option<(LegacyFile, Unreadable)>>| {
        match found {
            Ok((file, _)) | Err(Some((file, _))) => Some(file.clone()),
            Err(None) => None,
        }
    };
    let (settings_file, env_file) = (file_of(&settings), file_of(&env));
    let (used, unused) = match (settings, env) {
        (Ok((file, values)), _) if values.sets_any() => (Chosen::Values(file, values), env_file),
        (_, Ok((file, values))) => (Chosen::Values(file, values), settings_file),
        (Ok((file, values)), _) => (Chosen::Values(file, values), env_file),
        (Err(Some((file, why))), _) => (Chosen::Unreadable(file, why), env_file),
        (Err(None), Err(Some((file, why)))) => (Chosen::Unreadable(file, why), None),
        (Err(None), Err(None)) => return None,
    };
    Some(Choice { used, unused })
}

/// Adds the planned connections to the new file, each with its kind's default label; what was
/// left out of each is named by the connection it was for.
fn add_planned(
    file: &mut ConfigFile,
    plan: Plan,
) -> Result<(Vec<ConnectionId>, Vec<LegacyImportSkipDto>), super::ConfigEditError> {
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for planned in plan.connections {
        let id = file.add(&planned.source, None, planned.location)?;
        for (key, stored) in &planned.values {
            file.set(&id, *key, Some(stored))?;
        }
        skipped.extend(
            planned
                .skipped
                .into_iter()
                .map(|(field, reason)| LegacyImportSkipDto {
                    connection: Some(id.clone()),
                    field: field.to_string(),
                    reason,
                }),
        );
        imported.push(id);
    }
    Ok((imported, skipped))
}

fn dto(file: &LegacyFile) -> LegacyFileDto {
    LegacyFileDto {
        source: file.source,
        path: file.path.display().to_string(),
    }
}

/// A left-out value for people, by connection, key and reason, never by value.
fn skip_line(skip: &LegacyImportSkipDto) -> String {
    let why = match skip.reason {
        LegacySkipReason::Invalid => "today's settings do not accept it",
        LegacySkipReason::NoAddress => "it has no server address to be bound to",
        LegacySkipReason::InvalidWslRepo => "it is not Distro:/absolute/path",
        LegacySkipReason::ReadsEnvironment => {
            "its line refers to a variable, which the import never reads"
        }
    };
    match &skip.connection {
        Some(connection) => format!("{connection}: {} was not imported: {why}", skip.field),
        None => format!("An item of {} was not imported: {why}", skip.field),
    }
}
