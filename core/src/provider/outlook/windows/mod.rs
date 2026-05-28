use crate::platform::{ComGuard, create_outlook_application};

pub async fn check_outlook_ready() -> anyhow::Result<()> {
    tokio::task::spawn_blocking(check_outlook_ready_blocking)
        .await
        .map_err(|err| anyhow::anyhow!("Outlook startup check task failed: {err}"))?
}

fn check_outlook_ready_blocking() -> anyhow::Result<()> {
    let _com = ComGuard::new()?;
    let outlook = create_outlook_application()?;
    let namespace = outlook.method_dispatch_string("GetNamespace", "MAPI")?;
    let calendar = namespace.method_dispatch_i32("GetDefaultFolder", 9)?;
    let _items = calendar.property_dispatch("Items")?;
    Ok(())
}
