use crate::error::{Error, Result};
use crate::model::{Instance, JavaInfo, LaunchAuth, LaunchInfo, Settings};
use crate::mojang::{Rule, VersionJson, is_native, library_file, rules_allow};
use crate::paths;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn launch(
    instance: &Instance,
    version: &VersionJson,
    settings: &Settings,
    java: &JavaInfo,
    auth: &LaunchAuth,
) -> Result<LaunchInfo> {
    let natives = paths::instances().join(&instance.id).join("natives");
    let game_dir = paths::instances().join(&instance.id).join("game");
    std::fs::create_dir_all(&game_dir)?;
    std::fs::create_dir_all(&natives)?;

    let client_jar = if instance.loader.eq_ignore_ascii_case("forge") {
        crate::forge::client_jar(instance)?
    } else {
        paths::meta()
            .join("versions")
            .join(&version.id)
            .join("client.jar")
    };
    let classpath = build_classpath(version, &client_jar);
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", auth.name.clone());
    vars.insert("version_name", version.id.clone());
    vars.insert("game_directory", game_dir.to_string_lossy().to_string());
    vars.insert(
        "assets_root",
        paths::meta().join("assets").to_string_lossy().to_string(),
    );
    vars.insert("assets_index_name", version.assets.clone());
    vars.insert("auth_uuid", auth.uuid.clone());
    vars.insert("auth_access_token", auth.access_token.clone());
    vars.insert("clientid", auth.client_id.clone());
    vars.insert("auth_xuid", auth.xuid.clone());
    vars.insert("user_type", auth.user_type.clone());
    vars.insert(
        "version_type",
        version
            .kind
            .clone()
            .unwrap_or_else(|| "release".to_string()),
    );
    vars.insert("user_properties", "{}".to_string());
    vars.insert("natives_directory", natives.to_string_lossy().to_string());
    vars.insert(
        "library_directory",
        paths::meta()
            .join("libraries")
            .to_string_lossy()
            .to_string(),
    );
    vars.insert("launcher_name", "Bubanya".to_string());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").to_string());
    vars.insert("classpath", classpath.clone());
    vars.insert(
        "classpath_separator",
        if cfg!(windows) { ";" } else { ":" }.to_string(),
    );

    let mut command = Command::new(crate::process::launch_java(&java.path));
    command.current_dir(&game_dir);
    let ram_mb = instance.ram_mb.unwrap_or(settings.ram_mb);
    command.arg(format!("-Xmx{ram_mb}M"));
    command.arg(format!("-Xms{ram_mb}M"));

    let mut has_classpath = false;
    if let Some(arguments) = &version.arguments {
        for arg in &arguments.jvm {
            for expanded in expand_arg(arg, &vars) {
                if expanded == "-cp" || expanded == "-classpath" {
                    has_classpath = true;
                }
                command.arg(expanded);
            }
        }
    }
    if !has_classpath {
        command.arg(format!("-Djava.library.path={}", natives.display()));
        command.arg("-cp").arg(&classpath);
    }

    if let Some(extra) = &instance.jvm_args {
        for part in extra.split_whitespace() {
            if !part.is_empty() {
                command.arg(part);
            }
        }
    }

    command.arg(&version.main_class);

    if let Some(arguments) = &version.arguments {
        for arg in &arguments.game {
            for expanded in expand_arg(arg, &vars) {
                command.arg(expanded);
            }
        }
    } else if let Some(legacy) = &version.minecraft_arguments {
        for part in legacy.split_whitespace() {
            command.arg(expand(part, &vars));
        }
    }

    let log_dir = paths::instances().join(&instance.id).join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log = std::fs::File::create(log_dir.join("latest.log"))?;
    command.stdout(Stdio::from(log.try_clone()?));
    command.stderr(Stdio::from(log));

    let command_line = format!("{command:?}");
    let _ = std::fs::write(log_dir.join("launch.txt"), &command_line);
    let child = command
        .spawn()
        .map_err(|error| Error::Msg(format!("Не удалось запустить Java: {error}")))?;
    Ok(LaunchInfo {
        pid: child.id(),
        command: command_line,
    })
}

fn build_classpath(version: &VersionJson, client_jar: &Path) -> String {
    let mut entries = Vec::new();
    entries.push(client_jar.to_string_lossy().to_string());
    for library in &version.libraries {
        if !rules_allow(&library.rules) || is_native(library) {
            continue;
        }
        if let Some((_, path, _)) = library_file(library) {
            entries.push(
                paths::meta()
                    .join("libraries")
                    .join(path)
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    entries.join(if cfg!(windows) { ";" } else { ":" })
}

fn expand_arg(arg: &serde_json::Value, vars: &HashMap<&str, String>) -> Vec<String> {
    if let Some(text) = arg.as_str() {
        return vec![expand(text, vars)];
    }
    if let Some(object) = arg.as_object() {
        let rules: Vec<Rule> = object
            .get("rules")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        if !rules_allow(&rules) {
            return Vec::new();
        }
        if let Some(value) = object.get("value") {
            if let Some(text) = value.as_str() {
                return vec![expand(text, vars)];
            }
            if let Some(array) = value.as_array() {
                return array
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|text| expand(text, vars))
                    .collect();
            }
        }
    }
    Vec::new()
}

fn expand(text: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = text.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    result
}
