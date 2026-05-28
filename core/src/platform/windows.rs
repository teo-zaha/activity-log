use std::mem::{ManuallyDrop, size_of};

use anyhow::{Result, anyhow};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Utc};
use windows::{
    Win32::{
        Foundation::{
            ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, SYSTEMTIME, VARIANT_FALSE, VARIANT_TRUE,
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
            Ole::DISPID_PROPERTYPUT,
            Variant::{
                VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_BSTR, VT_DATE,
                VT_DISPATCH, VT_EMPTY, VT_I4, VariantChangeType, VariantClear, VariantInit,
                VariantTimeToSystemTime,
            },
        },
    },
    core::{BSTR, HRESULT, HSTRING, PCWSTR},
};

const OUTLOOK_APPLICATION: &str = "Outlook.Application";
const DISPATCH_BUFFER_BYTES: usize = 64 * 1024;

pub struct ComGuard;

impl ComGuard {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
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

pub fn create_outlook_application() -> Result<DispatchObject> {
    let prog_id = HSTRING::from(OUTLOOK_APPLICATION);
    let clsid = unsafe {
        CLSIDFromProgID(PCWSTR(prog_id.as_ptr()))
            .map_err(|err| anyhow!("Outlook COM registration unavailable: {err}"))?
    };

    let dispatch = unsafe {
        CoCreateInstance::<IDispatch>(&clsid, None, CLSCTX_LOCAL_SERVER)
            .map_err(|err| anyhow!("Outlook COM startup failed: {err}"))?
    };

    Ok(DispatchObject { dispatch })
}

pub fn system_log_access(log_name: &str) -> Result<()> {
    let query = EventHandle::query(log_name, "*")?;
    drop(query);
    Ok(())
}

pub fn query_windows_event_log(log_name: &str, query: &str) -> Result<Vec<String>> {
    let result_set = EventHandle::query(log_name, query)?;
    let mut event_handles = [0isize; 16];
    let mut xml_events = Vec::new();

    loop {
        let mut returned = 0;
        let next = unsafe { EvtNext(result_set.raw, &mut event_handles, 0, 0, &mut returned) };

        match next {
            Ok(()) => {
                for raw in &event_handles[..returned as usize] {
                    let event = EventHandle::from_raw(EVT_HANDLE(*raw));
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
    pub fn property_dispatch(&self, name: &str) -> Result<Self> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        Ok(Self {
            dispatch: result.into_dispatch(name)?,
        })
    }

    pub fn property_string(&self, name: &str) -> Result<String> {
        self.property_optional_string(name)?
            .ok_or_else(|| anyhow!("{name} was empty"))
    }

    pub fn property_optional_string(&self, name: &str) -> Result<Option<String>> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_optional_string()
    }

    pub fn property_i32(&self, name: &str) -> Result<i32> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_i32(name)
    }

    pub fn property_bool(&self, name: &str) -> Result<bool> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_bool(name)
    }

    pub fn property_rfc3339(&self, name: &str) -> Result<String> {
        let result = invoke_zero(&self.dispatch, name, DISPATCH_PROPERTYGET)?;
        result.into_rfc3339(name)
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
        let flags = (EvtQueryChannelPath | EvtQueryForwardDirection).0;
        let raw = unsafe {
            EvtQuery(
                None,
                PCWSTR(log_name.as_ptr()),
                PCWSTR(query.as_ptr()),
                flags,
            )
            .map_err(|err| anyhow!("Windows Event Log access failed: {err}"))?
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
        unsafe {
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
        }
        variant
    }

    fn from_i32(value: i32) -> Self {
        let mut variant = Self::empty();
        unsafe {
            variant.raw.Anonymous = VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I4,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { lVal: value },
                }),
            };
        }
        variant
    }

    fn from_bool(value: bool) -> Self {
        let mut variant = Self::empty();
        unsafe {
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
        }
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
        let value = unsafe { converted.raw.Anonymous.Anonymous.Anonymous.lVal };
        if converted.raw.Anonymous.Anonymous.vt != VT_I4 {
            return Err(anyhow!("Expected integer result from {name}"));
        }
        Ok(value)
    }

    fn into_bool(self, name: &str) -> Result<bool> {
        let converted = convert_variant(&self.raw, VT_BOOL)?;
        let value = unsafe { converted.raw.Anonymous.Anonymous.Anonymous.boolVal };
        if converted.raw.Anonymous.Anonymous.vt != VT_BOOL {
            return Err(anyhow!("Expected boolean result from {name}"));
        }
        Ok(value != VARIANT_FALSE)
    }

    fn into_rfc3339(self, name: &str) -> Result<String> {
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
        let local = Local
            .from_local_datetime(&naive)
            .single()
            .ok_or_else(|| anyhow!("Ambiguous Outlook local time for {name}"))?;

        Ok(local.with_timezone(&Utc).to_rfc3339())
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
                std::ptr::null(),
                0,
                flags,
                &params,
                Some(&mut result.raw),
                Some(&mut excep),
                Some(&mut arg_error),
            )
            .map_err(|err| anyhow!("Windows dispatch {name} failed: {err}"))?;
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
                std::ptr::null(),
                0,
                flags,
                &params,
                Some(&mut result.raw),
                Some(&mut excep),
                Some(&mut arg_error),
            )
            .map_err(|err| anyhow!("Windows dispatch {name} failed: {err}"))?;
    }

    Ok(result)
}

fn resolve_dispid(dispatch: &IDispatch, name: &str) -> Result<i32> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let name_ptr = PCWSTR(wide.as_ptr());
    let mut dispid = 0;

    unsafe {
        dispatch
            .GetIDsOfNames(std::ptr::null(), &name_ptr, 1, 0, &mut dispid)
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

pub fn local_date_bounds_as_systemtime(
    since: chrono::NaiveDate,
    until: chrono::NaiveDate,
) -> Result<(SYSTEMTIME, SYSTEMTIME)> {
    Ok((
        date_to_systemtime(since, 0, 0, 0)?,
        date_to_systemtime(until, 23, 59, 59)?,
    ))
}

fn date_to_systemtime(
    date: chrono::NaiveDate,
    hour: u16,
    minute: u16,
    second: u16,
) -> Result<SYSTEMTIME> {
    Ok(SYSTEMTIME {
        wYear: date
            .year()
            .try_into()
            .map_err(|_| anyhow!("invalid year"))?,
        wMonth: date
            .month()
            .try_into()
            .map_err(|_| anyhow!("invalid month"))?,
        wDayOfWeek: 0,
        wDay: date.day().try_into().map_err(|_| anyhow!("invalid day"))?,
        wHour: hour,
        wMinute: minute,
        wSecond: second,
        wMilliseconds: 0,
    })
}
