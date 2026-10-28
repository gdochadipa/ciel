//! Offline operator setup. No Google client, environment secrets, journal or listener.
use crate::domain::registry::Registry;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};
const MAX_BYTES: usize = 1024 * 1024;

fn validation_code(reason: &str) -> &'static str {
    match reason {
        "invalid registry bounds" => "registry_bounds",
        "invalid document registration" => "registry_invalid_document",
        "invalid spreadsheet registration" => "registry_invalid_spreadsheet",
        "grid writable range must be contained in a readable grid" => {
            "registry_invalid_grid_write_range"
        }
        "invalid bounded grid range" | "grid reads require a bounded read-only range" => {
            "registry_invalid_grid_range"
        }
        "invalid bounded view registration"
        | "invalid cell coordinate"
        | "invalid merge coordinate" => "registry_invalid_view",
        _ => "registry_invalid",
    }
}
fn load(path: &Path) -> Result<Registry, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "registry_unavailable")?;
    if !metadata.is_file() {
        return Err("registry_unsafe_file");
    }
    if metadata.len() > MAX_BYTES as u64 {
        return Err("registry_too_large");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| "registry_unavailable")?;
    if !file
        .metadata()
        .map_err(|_| "registry_unavailable")?
        .is_file()
    {
        return Err("registry_unsafe_file");
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "registry_unavailable")?;
    if bytes.len() > MAX_BYTES {
        return Err("registry_too_large");
    }
    let registry: Registry =
        serde_json::from_slice(&bytes).map_err(|error| match error.classify() {
            serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
                "registry_json_invalid"
            }
            _ => "registry_schema_invalid",
        })?;
    registry
        .validate()
        .map_err(|reason| validation_code(&reason))?;
    Ok(registry)
}
fn summary(registry: &Registry) -> Value {
    let readable = registry.spreadsheets.iter().filter(|s| s.readable);
    json!({"spreadsheets":registry.spreadsheets.len(),"documents":registry.documents.len(),
        "views":registry.spreadsheets.iter().map(|s|s.views.len()).sum::<usize>(),
        "readable_spreadsheets":readable.clone().count(),
        "readable_documents":registry.documents.iter().filter(|d|d.readable).count(),
        "configured_write_permissions":{
            "create_tabs":readable.clone().filter(|s|s.create_tabs).count(),
            "grid_ranges":readable.clone().flat_map(|s|&s.views).filter(|v|v.writable_range.is_some()).count(),
            "structured_views":readable.flat_map(|s|&s.views).filter(|v|v.structured()).count(),
            "documents":registry.documents.iter().filter(|d|d.writable).count()}})
}
fn initialize(path: &Path) -> Result<Registry, &'static str> {
    let mut registry: Registry =
        serde_json::from_str(include_str!("../../../../config/workspace.example.json"))
            .map_err(|_| "template_invalid")?;
    for spreadsheet in &mut registry.spreadsheets {
        spreadsheet.readable = false;
        spreadsheet.create_tabs = false;
        for view in &mut spreadsheet.views {
            view.writable_range = None;
            view.write_schema = None;
        }
    }
    for document in &mut registry.documents {
        document.readable = false;
        document.writable = false;
    }
    registry.validate().map_err(|_| "template_invalid")?;
    let mut bytes = serde_json::to_vec_pretty(&registry).map_err(|_| "template_invalid")?;
    bytes.push(b'\n');
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            "output_exists"
        } else {
            "output_unavailable"
        }
    })?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "output_write_failed")?;
    Ok(registry)
}
pub fn run(args: &[OsString]) -> (Value, i32) {
    if cfg!(not(unix)) {
        return (
            json!({"ok":false,"mode":"offline","error":{"code":"unsupported_platform"},"live_access_checked":false,"activation_changed":false}),
            1,
        );
    }
    let operation = if args.len() == 4
        && args[0] == "registry"
        && args[1] == "check"
        && args[2] == "--config"
    {
        load(Path::new(&args[3])).map(|registry| (registry, false))
    } else if args.len() == 4 && args[0] == "registry" && args[1] == "init" && args[2] == "--output"
    {
        initialize(Path::new(&args[3])).map(|registry| (registry, true))
    } else {
        return (
            json!({"ok":false,"mode":"offline","error":{"code":"usage","message":"Use registry check --config PATH or registry init --output PATH."},"live_access_checked":false,"activation_changed":false}),
            2,
        );
    };
    match operation {
        Ok((registry, changed)) => (
            json!({"ok":true,"mode":"offline","summary":summary(&registry),"configuration_changed":changed,"live_access_checked":false,"activation_changed":false}),
            0,
        ),
        Err(code) => (
            json!({"ok":false,"mode":"offline","error":{"code":code},"live_access_checked":false,"activation_changed":false}),
            1,
        ),
    }
}
