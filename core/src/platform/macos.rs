use anyhow::{Result, anyhow};
use objc2::AnyThread;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{
    NSAppleScript, NSAppleScriptErrorBriefMessage, NSAppleScriptErrorMessage, NSDictionary,
    NSString,
};

const OUTLOOK_BUNDLE_ID: &str = "com.microsoft.Outlook";

pub fn outlook_is_installed() -> bool {
    let workspace = NSWorkspace::sharedWorkspace();
    let bundle_id = NSString::from_str(OUTLOOK_BUNDLE_ID);
    workspace
        .URLForApplicationWithBundleIdentifier(&bundle_id)
        .is_some()
}

pub fn run_apple_script(source: &str) -> Result<String> {
    let source = NSString::from_str(source);
    let Some(script) = NSAppleScript::initWithSource(NSAppleScript::alloc(), &source) else {
        return Err(anyhow!("Failed to compile AppleScript source"));
    };

    let mut error_info = None;
    let descriptor = unsafe { script.executeAndReturnError(Some(&mut error_info)) };

    if let Some(message) = error_info
        .as_ref()
        .and_then(|info| apple_script_error_message(info))
        .filter(|message| !message.is_empty())
    {
        return Err(anyhow!(message));
    }

    let Some(value) = descriptor.stringValue() else {
        return Err(anyhow!("AppleScript did not return text output"));
    };

    Ok(value.to_string())
}

fn apple_script_error_message(info: &NSDictionary<NSString, AnyObject>) -> Option<String> {
    let brief = unsafe { NSAppleScriptErrorBriefMessage };
    let message = unsafe { NSAppleScriptErrorMessage };

    info.objectForKey(brief)
        .or_else(|| info.objectForKey(message))
        .and_then(|value| value.downcast::<NSString>().ok())
        .map(|value| value.to_string())
}
