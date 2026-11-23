use std::{
    mem::{ManuallyDrop, size_of},
    sync::atomic::AtomicBool,
};

use anyhow::{Result, anyhow};
use chrono::NaiveDate;
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, DISP_E_EXCEPTION, E_ACCESSDENIED, ERROR_INSUFFICIENT_BUFFER,
            ERROR_NO_MORE_ITEMS, MK_E_UNAVAILABLE, SYSTEMTIME, VARIANT_FALSE, VARIANT_TRUE,
        },
        System::{
            Com::{
                CLSCTX_LOCAL_SERVER, CLSIDFromProgID, COINIT_APARTMENTTHREADED, CoCreateInstance,
                CoInitializeEx, CoUninitialize, DISPATCH_FLAGS, DISPATCH_METHOD,
                DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO, IDispatch,
            },
            EventLog::{
                EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath,
                EvtQueryForwardDirection, EvtRender, EvtRenderEventXml,
            },
            Ole::{DISPID_PROPERTYPUT, GetActiveObject},
            Variant::{
                VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_BSTR, VT_DATE,
                VT_DISPATCH, VT_EMPTY, VT_I4, VariantChangeType, VariantClear, VariantInit,
                VariantTimeToSystemTime,
            },
        },
    },
    core::{BSTR, GUID, HRESULT, HSTRING, IUnknown, Interface, PCWSTR},
};

use crate::provider::source_failure::{SourceFailure, SourceProblem};

const OUTLOOK_APPLICATION: &str = "Outlook.Application";
const DISPATCH_BUFFER_BYTES: usize = 64 * 1024;
// IDispatch's reserved `riid` must point at IID_NULL; a null pointer is rejected
// by the RPC proxy for out-of-process servers like Outlook (0x800706F4).
const IID_NULL: GUID = GUID::zeroed();

pub struct ComGuard;

impl ComGuard {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|err| anyhow!("COM initialization failed: {err}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[derive(Clone)]
pub struct DispatchObject {
    dispatch: IDispatch,
}

fn outlook_clsid() -> Result<GUID> {
    let prog_id = HSTRING::from(OUTLOOK_APPLICATION);
    unsafe {
        CLSIDFromProgID(PCWSTR(prog_id.as_ptr()))
            .map_err(|err| anyhow!("Outlook COM registration unavailable: {err}"))
    }
}

pub fn outlook_is_installed() -> bool {
    outlook_clsid().is_ok()
}

/// The Outlook registered as running, if any. Outlook registers late after starting and only
/// for callers at its own integrity level, so `None` does not prove it is closed.
pub fn active_outlook_application() -> Result<Option<DispatchObject>> {
    let clsid = outlook_clsid()?;
    let mut unknown: Option<IUnknown> = None;
    match unsafe { GetActiveObject(&clsid, None, &mut unknown) } {
        Ok(()) => {}
        Err(err) if err.code() == MK_E_UNAVAILABLE => return Ok(None),
        Err(err) => return Err(anyhow!("Could not attach to Outlook: {err}")),
    }
    let unknown = unknown.ok_or_else(|| anyhow!("Outlook returned no COM object"))?;
    DispatchObject::from_unknown(&unknown).map(Some)
}

/// Asks COM for Outlook's application object. Outlook is single-instance, so while it runs this
/// returns the open instance; call it only then, as it would otherwise start Outlook.
pub fn connect_outlook_application() -> Result<DispatchObject> {
    let clsid = outlook_clsid()?;
    let unknown: IUnknown = unsafe { CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER) }
        .map_err(|err| anyhow!("{err}"))?;
    DispatchObject::from_unknown(&unknown)
}

/// Executable names of the running processes, such as `OUTLOOK.EXE`.
pub fn running_process_names() -> Vec<String> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut names = Vec::new();
    let mut next = unsafe { Process32FirstW(snapshot, &mut entry) };
    while next.is_ok() {
        let len = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        names.push(String::from_utf16_lossy(&entry.szExeFile[..len]));
        next = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    names
}

/// Whether this process runs elevated (as administrator).
pub fn process_is_elevated() -> bool {
    use windows::Win32::{
        Foundation::HANDLE,
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    let mut token = HANDLE::default();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0;
    let read = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    unsafe {
        let _ = CloseHandle(token);
    }
    read.is_ok() && elevation.TokenIsElevated != 0
}

/// Keeps subprocesses from flashing a console window in the GUI app.
pub fn hide_console_window(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Names of the WSL distributions registered for the current user, read from the registry so
/// that listing them never starts WSL.
pub fn wsl_distribution_names() -> Vec<String> {
    use windows::{
        Win32::{
            Foundation::ERROR_SUCCESS,
            System::Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW,
                RegGetValueW, RegOpenKeyExW,
            },
        },
        core::{PWSTR, w},
    };

    let mut lxss = HKEY::default();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Lxss"),
            Some(0),
            KEY_READ,
            &mut lxss,
        )
    };
    if opened != ERROR_SUCCESS {
        return Vec::new();
    }

    let mut names = Vec::new();
    for index in 0.. {
        // Subkeys are distribution GUIDs such as `{0a1b…}`; 256 UTF-16 units is ample.
        let mut subkey = [0u16; 256];
        let mut subkey_len = subkey.len() as u32;
        let listed = unsafe {
            RegEnumKeyExW(
                lxss,
                index,
                Some(PWSTR(subkey.as_mut_ptr())),
                &mut subkey_len,
                None,
                None,
                None,
                None,
            )
        };
        if listed != ERROR_SUCCESS {
            break;
        }

        let mut name = [0u16; 256];
        let mut name_bytes = (name.len() * size_of::<u16>()) as u32;
        let read = unsafe {
            RegGetValueW(
                lxss,
                PCWSTR(subkey.as_ptr()),
                w!("DistributionName"),
                RRF_RT_REG_SZ,
                None,
                Some(name.as_mut_ptr().cast()),
                Some(&mut name_bytes),
            )
        };
        if read == ERROR_SUCCESS {
            let units = (name_bytes as usize / size_of::<u16>()).saturating_sub(1);
            names.push(String::from_utf16_lossy(&name[..units]));
        }
    }

    unsafe {
        let _ = RegCloseKey(lxss);
    }
    names
}

/// Runs a POSIX shell script in a WSL distribution as a login shell, so `PATH` matches an
/// interactive terminal (NixOS keeps git outside `/usr/bin`). The script goes in on stdin, so
/// no value in it passes through Windows command-line quoting. One script at a time per
/// distribution; `wsl.exe` is killed once `cancelled` is set.
pub fn run_wsl_script(distro: &str, script: &str, cancelled: &AtomicBool) -> Result<String> {
    use std::sync::LazyLock;

    use super::blocking::{ExclusiveByName, run_cancellable};

    static DISTRIBUTIONS: LazyLock<ExclusiveByName> = LazyLock::new(ExclusiveByName::new);
    DISTRIBUTIONS.get(distro).run(cancelled, || {
        // `run_cancellable` starts it without a console window.
        let mut command = std::process::Command::new("wsl.exe");
        command.args(["-d", distro, "--exec", "/bin/sh", "-l", "-s"]);
        let output = run_cancellable(&mut command, Some(script.as_bytes().to_vec()), cancelled)
            .map_err(|err| anyhow!("wsl.exe: {err:#}"))?;
        if !output.status.success() {
            return Err(anyhow!(
                "wsl.exe exited with {}: {}",
                output.status,
                wsl_message(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })?
}

/// wsl.exe writes its own messages as UTF-16LE, while Linux programs write UTF-8.
fn wsl_message(bytes: &[u8]) -> String {
    let looks_utf16 = bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).all(|byte| *byte == 0);
    if looks_utf16 {
        let (pairs, _) = bytes.as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

pub fn system_log_access(log_name: &str) -> Result<()> {
    let query = EventHandle::query(log_name, "*")?;
    drop(query);
    Ok(())
}

/// The events of `log_name` that match `query`, as XML. One read at a time; between batches
/// it stops once `cancelled` is set.
pub fn query_windows_event_log(
    log_name: &str,
    query: &str,
    cancelled: &AtomicBool,
) -> Result<Vec<String>> {
    static EVENT_LOG: super::blocking::Exclusive = super::blocking::Exclusive::new();
    EVENT_LOG.run(cancelled, || read_event_log(log_name, query, cancelled))?
}

fn read_event_log(log_name: &str, query: &str, cancelled: &AtomicBool) -> Result<Vec<String>> {
    let result_set = EventHandle::query(log_name, query)?;
    let mut event_handles = [0isize; 16];
    let mut xml_events = Vec::new();

    loop {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow::Error::new(super::blocking::Stopped));
        }
        let mut returned = 0;
        let next = unsafe { EvtNext(result_set.raw, &mut event_handles, 0, 0, &mut returned) };

        match next {
            Ok(()) => {
                // Every handle of the batch is owned before the first render can fail, so an
                // error closes the rest too.
                let batch = event_handles[..returned as usize]
                    .iter()
                    .map(|raw| EventHandle::from_raw(EVT_HANDLE(*raw)))
                    .collect::<Vec<_>>();
                for event in &batch {
                    xml_events.push(render_event_xml(event.raw)?);
                }
            }
            Err(err) if err.code() == HRESULT::from_win32(ERROR_NO_MORE_ITEMS.0) => break,
            Err(err) => return Err(anyhow!("Windows Event Log query failed: {err}")),
        }
    }

    Ok(xml_events)
}

impl DispatchObject {
    fn from_unknown(unknown: &IUnknown) -> Result<Self> {
        let dispatch = unknown
            .cast::<IDispatch>()
            .map_err(|err| anyhow!("Outlook COM object has no dispatch interface: {err}"))?;
        Ok(Self { dispatch })
    }

    pub fn property_dispatch(&self, name: &str) -> Result<Self> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        Ok(Self {
            dispatch: result.into_dispatch(name)?,
        })
    }

    pub fn property_optional_string(&self, name: &str) -> Result<Option<String>> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_optional_string()
    }

    pub fn property_i32(&self, name: &str) -> Result<i32> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_i32(name)
    }

    /// Reads a COM date that holds UTC wall-clock time, such as Outlook's `StartUTC`.
    pub fn property_utc_rfc3339(&self, name: &str) -> Result<String> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_utc_rfc3339(name)
    }

    /// Calls a method without arguments that returns an object or `Nothing`, such as
    /// `Items.GetNext` at the end of a collection.
    pub fn method_optional_dispatch(&self, name: &str) -> Result<Option<Self>> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_METHOD)?;
        Ok(result
            .into_optional_dispatch()
            .map(|dispatch| Self { dispatch }))
    }

    pub fn method_dispatch_i32(&self, name: &str, value: i32) -> Result<Self> {
        let mut value = OwnedVariant::from_i32(value);
        let result = invoke_one(&self.dispatch, name, DISPATCH_METHOD, &mut value, false)?;
        Ok(Self {
            dispatch: result.into_dispatch(name)?,
        })
    }

    pub fn method_dispatch_string(&self, name: &str, value: &str) -> Result<Self> {
        let mut value = OwnedVariant::from_string(value);
        let result = invoke_one(&self.dispatch, name, DISPATCH_METHOD, &mut value, false)?;
        Ok(Self {
            dispatch: result.into_dispatch(name)?,
        })
    }

    pub fn method_unit_string(&self, name: &str, value: &str) -> Result<()> {
        let mut value = OwnedVariant::from_string(value);
        let _ = invoke_one(&self.dispatch, name, DISPATCH_METHOD, &mut value, false)?;
        Ok(())
    }

    pub fn set_bool(&self, name: &str, value: bool) -> Result<()> {
        let mut value = OwnedVariant::from_bool(value);
        let _ = invoke_one(&self.dispatch, name, DISPATCH_PROPERTYPUT, &mut value, true)?;
        Ok(())
    }
}

struct EventHandle {
    raw: EVT_HANDLE,
}

impl EventHandle {
    fn query(log_name: &str, query: &str) -> Result<Self> {
        let log_name = HSTRING::from(log_name);
        let query = HSTRING::from(query);
        let flags = EvtQueryChannelPath.0 | EvtQueryForwardDirection.0;
        let raw = unsafe {
            EvtQuery(
                None,
                PCWSTR(log_name.as_ptr()),
                PCWSTR(query.as_ptr()),
                flags,
            )
            .map_err(|err| {
                let message = format!("Windows Event Log access failed: {err}");
                if err.code() == E_ACCESSDENIED {
                    anyhow::Error::new(SourceFailure::new(SourceProblem::PermissionDenied, message))
                } else {
                    anyhow!(message)
                }
            })?
        };

        Ok(Self { raw })
    }

    fn from_raw(raw: EVT_HANDLE) -> Self {
        Self { raw }
    }
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        if !self.raw.is_invalid() {
            unsafe {
                let _ = EvtClose(self.raw);
            }
        }
    }
}

struct OwnedVariant {
    raw: VARIANT,
}

impl OwnedVariant {
    fn empty() -> Self {
        Self {
            raw: unsafe { VariantInit() },
        }
    }

    fn from_string(value: &str) -> Self {
        let mut variant = Self::empty();
        variant.raw.Anonymous = VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_BSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    bstrVal: ManuallyDrop::new(BSTR::from(value)),
                },
            }),
        };
        variant
    }

    fn from_i32(value: i32) -> Self {
        let mut variant = Self::empty();
        variant.raw.Anonymous = VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: value },
            }),
        };
        variant
    }

    fn from_bool(value: bool) -> Self {
        let mut variant = Self::empty();
        variant.raw.Anonymous = VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_BOOL,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    boolVal: if value { VARIANT_TRUE } else { VARIANT_FALSE },
                },
            }),
        };
        variant
    }

    fn into_dispatch(self, name: &str) -> Result<IDispatch> {
        let dispatch = unsafe {
            if self.raw.Anonymous.Anonymous.vt != VT_DISPATCH {
                return Err(anyhow!("Expected dispatch result from {name}"));
            }

            let raw = &self.raw.Anonymous.Anonymous.Anonymous.pdispVal
                as *const ManuallyDrop<Option<IDispatch>>
                as *const Option<IDispatch>;
            (*raw)
                .clone()
                .ok_or_else(|| anyhow!("Expected dispatch pointer from {name}"))?
        };

        Ok(dispatch)
    }

    fn into_optional_dispatch(self) -> Option<IDispatch> {
        unsafe {
            if self.raw.Anonymous.Anonymous.vt != VT_DISPATCH {
                return None;
            }
            let raw = &self.raw.Anonymous.Anonymous.Anonymous.pdispVal
                as *const ManuallyDrop<Option<IDispatch>>
                as *const Option<IDispatch>;
            (*raw).clone()
        }
    }

    fn into_optional_string(self) -> Result<Option<String>> {
        let variant_type = unsafe { self.raw.Anonymous.Anonymous.vt };
        if variant_type == VT_EMPTY {
            return Ok(None);
        }

        let converted = convert_variant(&self.raw, VT_BSTR)?;
        let text = unsafe {
            let raw = &converted.raw.Anonymous.Anonymous.Anonymous.bstrVal
                as *const ManuallyDrop<BSTR> as *const BSTR;
            (*raw).to_string()
        };

        Ok(Some(text))
    }

    fn into_i32(self, name: &str) -> Result<i32> {
        let converted = convert_variant(&self.raw, VT_I4)?;
        let inner = unsafe { &converted.raw.Anonymous.Anonymous };
        if inner.vt != VT_I4 {
            return Err(anyhow!("Expected integer result from {name}"));
        }
        let value = unsafe { inner.Anonymous.lVal };
        Ok(value)
    }

    fn into_utc_rfc3339(self, name: &str) -> Result<String> {
        let converted = convert_variant(&self.raw, VT_DATE)?;
        let date = unsafe { converted.raw.Anonymous.Anonymous.Anonymous.date };
        let mut system_time = SYSTEMTIME::default();
        let ok = unsafe { VariantTimeToSystemTime(date, &mut system_time) };
        if ok == 0 {
            return Err(anyhow!("Failed to decode Outlook date from {name}"));
        }

        let date = NaiveDate::from_ymd_opt(
            system_time.wYear as i32,
            system_time.wMonth as u32,
            system_time.wDay as u32,
        )
        .ok_or_else(|| anyhow!("Invalid Outlook date components for {name}"))?;
        let naive = date
            .and_hms_milli_opt(
                system_time.wHour as u32,
                system_time.wMinute as u32,
                system_time.wSecond as u32,
                system_time.wMilliseconds as u32,
            )
            .ok_or_else(|| anyhow!("Invalid Outlook time components for {name}"))?;
        Ok(naive.and_utc().to_rfc3339())
    }
}

impl Drop for OwnedVariant {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.raw);
        }
    }
}

fn invoke_zero(dispatch: &IDispatch, name: &str, flags: DISPATCH_FLAGS) -> Result<OwnedVariant> {
    let dispid = resolve_dispid(dispatch, name)?;
    let params = DISPPARAMS::default();
    let mut result = OwnedVariant::empty();
    let mut excep = EXCEPINFO::default();
    let mut arg_error = 0;

    unsafe {
        dispatch
            .Invoke(
                dispid,
                &IID_NULL,
                0,
                flags,
                &params,
                Some(&mut result.raw),
                Some(&mut excep),
                Some(&mut arg_error),
            )
            .map_err(|err| dispatch_error(name, &err, &mut excep))?;
    }

    Ok(result)
}

fn invoke_one(
    dispatch: &IDispatch,
    name: &str,
    flags: DISPATCH_FLAGS,
    value: &mut OwnedVariant,
    property_put: bool,
) -> Result<OwnedVariant> {
    let dispid = resolve_dispid(dispatch, name)?;
    let mut named_arg = DISPID_PROPERTYPUT;
    let mut result = OwnedVariant::empty();
    let mut excep = EXCEPINFO::default();
    let mut arg_error = 0;
    let params = DISPPARAMS {
        rgvarg: &mut value.raw,
        rgdispidNamedArgs: if property_put {
            &mut named_arg
        } else {
            std::ptr::null_mut()
        },
        cArgs: 1,
        cNamedArgs: if property_put { 1 } else { 0 },
    };

    unsafe {
        dispatch
            .Invoke(
                dispid,
                &IID_NULL,
                0,
                flags,
                &params,
                Some(&mut result.raw),
                Some(&mut excep),
                Some(&mut arg_error),
            )
            .map_err(|err| dispatch_error(name, &err, &mut excep))?;
    }

    Ok(result)
}

fn dispatch_error(name: &str, err: &windows::core::Error, excep: &mut EXCEPINFO) -> anyhow::Error {
    // The callee allocates these BSTRs only for DISP_E_EXCEPTION; we own them.
    let description = unsafe {
        let description = ManuallyDrop::take(&mut excep.bstrDescription).to_string();
        drop(ManuallyDrop::take(&mut excep.bstrSource));
        drop(ManuallyDrop::take(&mut excep.bstrHelpFile));
        description
    };

    if err.code() == DISP_E_EXCEPTION && !description.is_empty() {
        anyhow!("Windows dispatch {name} failed: {description}")
    } else {
        anyhow!("Windows dispatch {name} failed: {err}")
    }
}

fn resolve_dispid(dispatch: &IDispatch, name: &str) -> Result<i32> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let name_ptr = PCWSTR(wide.as_ptr());
    let mut dispid = 0;

    unsafe {
        dispatch
            .GetIDsOfNames(&IID_NULL, &name_ptr, 1, 0, &mut dispid)
            .map_err(|err| anyhow!("Windows dispatch lookup for {name} failed: {err}"))?;
    }

    Ok(dispid)
}

fn convert_variant(
    source: &VARIANT,
    target: windows::Win32::System::Variant::VARENUM,
) -> Result<OwnedVariant> {
    let mut converted = OwnedVariant::empty();
    unsafe {
        VariantChangeType(&mut converted.raw, source, Default::default(), target)
            .map_err(|err| anyhow!("Variant conversion failed: {err}"))?;
    }
    Ok(converted)
}

fn render_event_xml(event: EVT_HANDLE) -> Result<String> {
    let mut buffer = vec![0u16; DISPATCH_BUFFER_BYTES / size_of::<u16>()];
    let mut used = 0;
    let mut property_count = 0;

    let render = unsafe {
        EvtRender(
            None,
            event,
            EvtRenderEventXml.0,
            DISPATCH_BUFFER_BYTES as u32,
            Some(buffer.as_mut_ptr().cast()),
            &mut used,
            &mut property_count,
        )
    };

    match render {
        Ok(()) => {}
        Err(err) if err.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0) => {
            return Err(anyhow!(
                "Windows Event Log event XML exceeded render buffer"
            ));
        }
        Err(err) => return Err(anyhow!("Windows Event Log render failed: {err}")),
    }

    let used_utf16 = used as usize / size_of::<u16>();
    if used_utf16 == 0 {
        return Ok(String::new());
    }

    Ok(String::from_utf16_lossy(
        &buffer[..used_utf16.saturating_sub(1)],
    ))
}

/// The Roaming AppData folder as Windows knows it for this account, whatever `APPDATA` says, so
/// every process of the user agrees on it; `None` when the shell cannot say.
pub(crate) fn roaming_app_data() -> Option<std::path::PathBuf> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_RoamingAppData, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath},
    };
    // SAFETY: on success the shell returns a NUL-terminated string it allocated, freed below.
    unsafe {
        let path =
            SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DONT_VERIFY, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0.cast_const().cast()));
        text.filter(|text| !text.is_empty())
            .map(std::path::PathBuf::from)
    }
}
