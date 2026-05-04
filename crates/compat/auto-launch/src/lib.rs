use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl Error {
    fn new<T: Into<String>>(message: T) -> Self {
        Self(message.into())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LinuxLaunchMode {
    Systemd,
}
#[derive(Clone, Copy, Debug)]
pub enum MacOSLaunchMode {
    LaunchAgent,
}
#[derive(Clone, Copy, Debug)]
pub enum WindowsEnableMode {
    CurrentUser,
}

#[derive(Debug, Clone, Default)]
pub struct AutoLaunchBuilder {
    app_name: String,
    app_path: String,
    args: Vec<String>,
}
impl AutoLaunchBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_app_name(&mut self, value: &str) -> &mut Self {
        self.app_name = value.into();
        self
    }
    pub fn set_app_path(&mut self, value: &str) -> &mut Self {
        self.app_path = value.into();
        self
    }
    pub fn set_args(&mut self, value: &[String]) -> &mut Self {
        self.args = value.to_vec();
        self
    }
    pub fn set_macos_launch_mode(&mut self, _: MacOSLaunchMode) -> &mut Self {
        self
    }
    pub fn set_agent_extra_config(&mut self, _: &str) -> &mut Self {
        self
    }
    pub fn set_windows_enable_mode(&mut self, _: WindowsEnableMode) -> &mut Self {
        self
    }
    pub fn set_linux_launch_mode(&mut self, _: LinuxLaunchMode) -> &mut Self {
        self
    }
    pub fn build(&self) -> Result<AutoLaunch> {
        if self.app_name.trim().is_empty() {
            return Err(Error::new("auto-launch app name is empty"));
        }
        if self.app_path.trim().is_empty() {
            return Err(Error::new("auto-launch app path is empty"));
        }
        Ok(AutoLaunch {
            app_name: self.app_name.clone(),
            app_path: self.app_path.clone(),
            args: self.args.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AutoLaunch {
    app_name: String,
    app_path: String,
    args: Vec<String>,
}
impl AutoLaunch {
    pub fn is_support() -> bool {
        cfg!(windows) || cfg!(target_os = "macos") || cfg!(target_os = "linux")
    }
    pub fn enable(&self) -> Result<()> {
        let path = self.launch_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::new(format!("failed to create {}: {e}", parent.display())))?;
        }
        fs::write(&path, self.launch_file_body())
            .map_err(|e| Error::new(format!("failed to write {}: {e}", path.display())))
    }
    pub fn disable(&self) -> Result<()> {
        let path = self.launch_file_path()?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::new(format!(
                "failed to remove {}: {e}",
                path.display()
            ))),
        }
    }
    pub fn is_enabled(&self) -> Result<bool> {
        Ok(self.launch_file_path()?.is_file())
    }
    fn launch_file_path(&self) -> Result<PathBuf> {
        let name = safe_identifier(&self.app_name);
        #[cfg(target_os = "linux")]
        {
            let base = env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| home_dir().map(|h| h.join(".config")))
                .ok_or_else(|| Error::new("failed to resolve home directory"))?;
            return Ok(base
                .join("systemd")
                .join("user")
                .join(format!("{}.service", name.to_ascii_lowercase())));
        }
        #[cfg(target_os = "macos")]
        {
            let home = home_dir().ok_or_else(|| Error::new("failed to resolve home directory"))?;
            return Ok(home
                .join("Library")
                .join("LaunchAgents")
                .join(format!("com.mcpace.{}.plist", name.to_ascii_lowercase())));
        }
        #[cfg(windows)]
        {
            let base = env::var_os("APPDATA")
                .map(PathBuf::from)
                .or_else(|| home_dir().map(|h| h.join("AppData").join("Roaming")))
                .ok_or_else(|| Error::new("failed to resolve APPDATA"))?;
            return Ok(base
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
                .join(format!("{name}.cmd")));
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            let _ = name;
            Err(Error::new("auto-launch is not supported on this target OS"))
        }
    }
    fn launch_file_body(&self) -> String {
        #[cfg(target_os = "linux")]
        {
            return format!("[Unit]\nDescription={} autostart\n\n[Service]\nType=simple\nExecStart={}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n", self.app_name, shell_command(&self.app_path, &self.args));
        }
        #[cfg(target_os = "macos")]
        {
            let args = std::iter::once(&self.app_path)
                .chain(self.args.iter())
                .map(|a| format!("    <string>{}</string>", xml_escape(a)))
                .collect::<Vec<_>>()
                .join("\n");
            return format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict><key>Label</key><string>com.mcpace.{}</string><key>ProgramArguments</key><array>\n{}\n</array><key>RunAtLoad</key><true/></dict></plist>\n", safe_identifier(&self.app_name).to_ascii_lowercase(), args);
        }
        #[cfg(windows)]
        {
            return format!(
                "@echo off\r\nstart \"{}\" {}\r\n",
                self.app_name,
                windows_command(&self.app_path, &self.args)
            );
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            String::new()
        }
    }
}
fn safe_identifier(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "app".into()
    } else {
        trimmed.into()
    }
}
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shell_command(app: &str, args: &[String]) -> String {
    std::iter::once(app.to_string())
        .chain(args.iter().cloned())
        .map(|v| shell_quote(&v))
        .collect::<Vec<_>>()
        .join(" ")
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '='))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
#[cfg(windows)]
fn windows_command(app: &str, args: &[String]) -> String {
    std::iter::once(app.to_string())
        .chain(args.iter().cloned())
        .map(|v| windows_quote(&v))
        .collect::<Vec<_>>()
        .join(" ")
}
#[cfg(windows)]
fn windows_quote(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\')
    {
        value.into()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}
