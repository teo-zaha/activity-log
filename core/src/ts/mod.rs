use strum::IntoEnumIterator;
use ts_rs::{Config, TS};

use crate::{
    ipc::{DESKTOP_COMMANDS, DESKTOP_EVENTS},
    menu::{INSTALL_MENU_COMMAND, MENU_COMMAND_EVENT, UPDATE_MENU_COMMAND},
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, ActivityState, ActivityUpdate,
        AddConnectionRequestDto, AddConnectionResponseDto, BackendError, BackendErrorKind,
        BusyStatus, CalendarEntry, ChangeStack, ChangeStackAbove, ChangeStackBottom,
        ChangeStackEnd, ChangeStackNode, CollectAnswer, CollectOptions, CollectRequest,
        CollectedActivitySummary, CollectionFailedResponse, ConnectionCount, ConnectionId,
        DescriptionBlock, ExportFile, ExportOutcome, ExportRequest, GitEntry, GitLabEntry,
        GitLabType, InstallReceipt, JiraEntry, JiraEntryType, ListItem, LogEntry, LogLevel,
        LogsResponse, MaximizeButtonState, MeetingWeight, MenuCommand, MenuInstall, MenuItemKey,
        MenuItemSpec, MenuItemState, MenuItemUpdate, MenuKey, MenuSection, MenuSpec, MenuSubmenu,
        OutlookEntry, OutputFormat, PartStamp, ProviderConfigConnectionStateDto,
        ProviderConfigFieldIssueDto, ProviderConfigFieldRefDto, ProviderConfigFieldStateDto,
        ProviderConfigFieldValueDto, ProviderConfigIssueKindDto, ProviderConfigSaveRequestDto,
        ProviderConfigSaveResponseDto, ProviderConfigSaveValueDto, ProviderConfigSnapshotDto,
        ProviderConfigValueDto, ProviderConfigValueTypeDto, ProviderInfo, ProviderKindDto,
        ProviderProblem, ProviderStatus, ProvidersResponse, ProvidersUpdate, RecheckAnswer,
        RemoveConnectionRequestDto, RuntimePing, RuntimeState, ShownActivity, Source, StackCut,
        StackedChange, SupportedOs, SystemEntry, SystemEntryType, TextSegment,
    },
    runtime::{PING_INTERVAL, forward::RuntimeMessage},
};

pub fn activity_typescript() -> String {
    let cfg = Config::default();
    let declarations = [
        export_decl::<Source>(&cfg),
        export_decl::<ConnectionId>(&cfg),
        export_decl::<SupportedOs>(&cfg),
        export_decl::<ProviderStatus>(&cfg),
        export_decl::<ProviderProblem>(&cfg),
        export_decl::<ProviderInfo>(&cfg),
        export_decl::<ProvidersResponse>(&cfg),
        export_decl::<ProviderConfigValueTypeDto>(&cfg),
        export_decl::<ProviderConfigFieldValueDto>(&cfg),
        export_decl::<ProviderConfigFieldStateDto>(&cfg),
        export_decl::<ProviderConfigConnectionStateDto>(&cfg),
        export_decl::<ProviderKindDto>(&cfg),
        export_decl::<ProviderConfigSnapshotDto>(&cfg),
        export_decl::<ProviderConfigValueDto>(&cfg),
        export_decl::<ProviderConfigSaveValueDto>(&cfg),
        export_decl::<ProviderConfigSaveRequestDto>(&cfg),
        export_decl::<ProviderConfigSaveResponseDto>(&cfg),
        export_decl::<ProviderConfigFieldRefDto>(&cfg),
        export_decl::<ProviderConfigIssueKindDto>(&cfg),
        export_decl::<ProviderConfigFieldIssueDto>(&cfg),
        export_decl::<AddConnectionRequestDto>(&cfg),
        export_decl::<RemoveConnectionRequestDto>(&cfg),
        export_decl::<AddConnectionResponseDto>(&cfg),
        export_decl::<GitLabType>(&cfg),
        export_decl::<JiraEntryType>(&cfg),
        export_decl::<SystemEntryType>(&cfg),
        export_decl::<BusyStatus>(&cfg),
        export_decl::<MeetingWeight>(&cfg),
        export_decl::<TextSegment>(&cfg),
        export_decl::<ListItem>(&cfg),
        export_decl::<DescriptionBlock>(&cfg),
        export_decl::<StackCut>(&cfg),
        export_decl::<StackedChange>(&cfg),
        export_decl::<ChangeStackBottom>(&cfg),
        export_decl::<ChangeStackEnd>(&cfg),
        export_decl::<ChangeStackNode>(&cfg),
        export_decl::<ChangeStackAbove>(&cfg),
        export_decl::<ChangeStack>(&cfg),
        export_decl::<GitLabEntry>(&cfg),
        export_decl::<GitEntry>(&cfg),
        export_decl::<JiraEntry>(&cfg),
        export_decl::<OutlookEntry>(&cfg),
        export_decl::<SystemEntry>(&cfg),
        export_decl::<CalendarEntry>(&cfg),
        export_decl::<ActivityEntry>(&cfg),
        export_decl::<ActivityLogMeta>(&cfg),
        export_decl::<ConnectionCount>(&cfg),
        export_decl::<CollectedActivitySummary>(&cfg),
        export_decl::<ActivityLog>(&cfg),
        export_decl::<CollectRequest>(&cfg),
        export_decl::<CollectionFailedResponse>(&cfg),
        export_decl::<BackendErrorKind>(&cfg),
        export_decl::<BackendError>(&cfg),
        export_decl::<OutputFormat>(&cfg),
        export_decl::<ExportRequest>(&cfg),
        export_decl::<ExportFile>(&cfg),
        export_decl::<ExportOutcome>(&cfg),
        export_decl::<LogLevel>(&cfg),
        export_decl::<LogEntry>(&cfg),
        export_decl::<LogsResponse>(&cfg),
        export_decl::<ActivityState>(&cfg),
        export_decl::<PartStamp>(&cfg),
        export_decl::<ActivityUpdate>(&cfg),
        export_decl::<ProvidersUpdate>(&cfg),
        export_decl::<RuntimeState>(&cfg),
        export_decl::<RuntimePing>(&cfg),
        export_decl::<InstallReceipt>(&cfg),
        export_decl::<ShownActivity>(&cfg),
        export_decl::<CollectAnswer>(&cfg),
        export_decl::<RecheckAnswer>(&cfg),
        export_decl::<MaximizeButtonState>(&cfg),
        export_decl::<MenuSubmenu>(&cfg),
        export_decl::<MenuKey>(&cfg),
        export_decl::<MenuItemKey>(&cfg),
        export_decl::<MenuItemState>(&cfg),
        export_decl::<MenuItemSpec>(&cfg),
        export_decl::<MenuSection>(&cfg),
        export_decl::<MenuSpec>(&cfg),
        export_decl::<MenuItemUpdate>(&cfg),
        export_decl::<MenuInstall>(&cfg),
        export_decl::<MenuCommand>(&cfg),
    ];

    let mut output =
        String::from("// This file is generated by `cargo run -p cli --bin generate-ts --`.\n\n");
    output.push_str(&declarations.join("\n\n"));
    output.push_str("\n\n");
    output.push_str(&constants());
    output
}

/// Values the dashboard checks the way the backend does, so it never has its own copy.
fn constants() -> String {
    let kinds = wire_names(BackendErrorKind::iter());
    let sources = wire_names(Source::iter());
    let submenus = wire_names(MenuSubmenu::iter());
    format!(
        "/** The longest range the backend collects or exports, in days (`CollectOptions::MAX_DAYS`). */\n\
         export const COLLECT_MAX_DAYS = {}\n\n\
         /** Every `BackendErrorKind`, for telling a backend error body from any other. */\n\
         export const BACKEND_ERROR_KINDS = [{kinds}] as const satisfies readonly BackendErrorKind[]\n\n\
         /** Every source kind, in the backend's order. */\n\
         export const SOURCES = [{sources}] as const satisfies readonly Source[]\n\n\
         /** How often `GET /api/events` sends its `ping` event, in seconds (`runtime::PING_INTERVAL`). */\n\
         export const RUNTIME_PING_SECONDS = {}\n\n\
         /** The desktop's event for a chosen menu item, a `MenuCommand`. */\n\
         export const MENU_COMMAND_EVENT = {}\n\n\
         /** Every submenu of the macOS menu bar, in the bar's order. */\n\
         export const MENU_SUBMENUS = [{submenus}] as const satisfies readonly MenuSubmenu[]\n\n\
         /** The desktop commands that install the macOS menu and change what its items show. */\n\
         export const MENU_COMMANDS = {{ install: {}, update: {} }} as const\n\n\
         /** The desktop app's commands, by the names it registers them under (`core::ipc`). */\n\
         export const DESKTOP_COMMANDS = {} as const\n\n\
         /** The desktop app's own events (`core::ipc`); the host's state comes as `RUNTIME_EVENTS`. */\n\
         export const DESKTOP_EVENTS = {} as const\n\n\
         /** The events the host's state comes as, on `GET /api/events` and from the desktop app. */\n\
         export const RUNTIME_EVENTS = [{}] as const\n",
        CollectOptions::MAX_DAYS,
        PING_INTERVAL.as_secs(),
        json(MENU_COMMAND_EVENT),
        json(INSTALL_MENU_COMMAND),
        json(UPDATE_MENU_COMMAND),
        object(DESKTOP_COMMANDS),
        object(DESKTOP_EVENTS),
        RuntimeMessage::EVENTS.map(json).join(", "),
    )
}

/// `{ key: "name", … }`.
fn object(names: &[(&str, &str)]) -> String {
    let fields = names
        .iter()
        .map(|(key, name)| format!("{key}: {}", json(name)))
        .collect::<Vec<_>>();
    format!("{{ {} }}", fields.join(", "))
}

fn json(text: &str) -> String {
    serde_json::to_string(text).expect("a string serializes")
}

/// The values as they are sent, comma-separated.
fn wire_names<T: serde::Serialize>(values: impl Iterator<Item = T>) -> String {
    values
        .map(|value| serde_json::to_string(&value).expect("an enum value serializes"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn export_decl<T: TS>(cfg: &Config) -> String {
    format!("export {}", T::decl(cfg))
}

#[cfg(test)]
mod tests;
