//! Host 子进程管理：拉起 `dsh web`，从 stdout 解析地址与 token，退出时回收。
//!
//! 上游事实（`packages/bundle/web-app/src/index.ts`）：
//! `dsh web` 启动完成后打印一行 `dsh web: http://127.0.0.1:<port>/?token=<launchToken>`。
//!
//! 设计要点：
//! - 用 `--port 0` 让 OS 分配端口，天然避免端口冲突；
//! - `--no-open` 阻止 Host 自己拉起浏览器；
//! - 解析成功后必须**继续把 stdout 读空**，否则管道写满会让 Host 阻塞。

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use reqwest::Url;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::TransportError;

/// 启动行的固定前缀。
pub const LAUNCH_LINE_PREFIX: &str = "dsh web: ";

/// 从启动行解析出的连接信息。
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchInfo {
    /// 形如 `http://127.0.0.1:3080/`（已去掉 query）。
    pub base_url: Url,
    /// 本次进程的 launch token。
    pub token: String,
}

/// 解析 `dsh web: http://127.0.0.1:3080/?token=…`。
///
/// 容忍前缀前有其它输出，也容忍 URL 后面跟着别的文字。
pub fn parse_launch_line(line: &str) -> Option<LaunchInfo> {
    let line = line.trim();
    let candidate = match line.find(LAUNCH_LINE_PREFIX) {
        Some(idx) => &line[idx + LAUNCH_LINE_PREFIX.len()..],
        None => line,
    };
    let url_str = candidate.split_whitespace().next()?;
    let url = Url::parse(url_str).ok()?;

    let token = url
        .query_pairs()
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.into_owned())?;
    if token.is_empty() {
        return None;
    }

    let mut base_url = url;
    base_url.set_query(None);
    base_url.set_fragment(None);

    Some(LaunchInfo { base_url, token })
}

/// 拉起 Host 的参数。
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// `dsh` 可执行文件路径（默认从 PATH 找）。
    pub bin: PathBuf,
    /// 追加参数；默认 `web --no-open --port 0`。
    pub args: Vec<String>,
    /// 等待启动行的上限。
    pub startup_timeout: Duration,
}

impl Default for HostConfig {
    /// `DSH_BIN` 可覆盖可执行文件（开发时常用 `./scripts/dev-host.sh`）。
    fn default() -> Self {
        let bin = std::env::var_os("DSH_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("dsh"));
        Self {
            bin,
            args: ["web", "--no-open", "--port", "0"]
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            startup_timeout: Duration::from_secs(40),
        }
    }
}

/// 持有 Host 子进程；drop 时终止（`kill_on_drop`）。
pub struct HostSupervisor {
    child: Child,
    info: LaunchInfo,
}

impl HostSupervisor {
    /// 启动 Host 并等待启动行。
    pub async fn spawn(config: HostConfig) -> Result<Self, TransportError> {
        let mut child = Command::new(&config.bin)
            .args(&config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                TransportError::HostStartup(format!("无法执行 {}: {err}", config.bin.display()))
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TransportError::HostStartup("stdout 不可用".into()))?;
        let mut lines = BufReader::new(stdout).lines();

        let info = tokio::time::timeout(config.startup_timeout, async {
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if let Some(info) = parse_launch_line(&line) {
                            return Ok(info);
                        }
                    }
                    Ok(None) => {
                        return Err(TransportError::HostStartup(
                            "Host 在打印启动地址前退出了".into(),
                        ));
                    }
                    Err(err) => return Err(TransportError::Io(err)),
                }
            }
        })
        .await
        .map_err(|_| TransportError::HostStartup("等待启动地址超时".into()))??;

        // 继续消费 stdout，避免 Host 因管道写满而卡住
        tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

        Ok(Self { child, info })
    }

    pub fn launch_info(&self) -> &LaunchInfo {
        &self.info
    }

    pub fn base_url(&self) -> &Url {
        &self.info.base_url
    }

    pub fn token(&self) -> &str {
        &self.info.token
    }

    /// 终止 Host：先等 2s 优雅退出，超时后强杀。
    pub async fn shutdown(mut self) {
        if let Err(err) = self.child.start_kill() {
            tracing::warn!(%err, "终止 Host 失败");
            return;
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_launch_line() {
        let info =
            parse_launch_line("dsh web: http://127.0.0.1:3080/?token=abc123").expect("应解析成功");
        assert_eq!(info.token, "abc123");
        assert_eq!(info.base_url.as_str(), "http://127.0.0.1:3080/");
    }

    #[test]
    fn parses_os_assigned_port() {
        let info =
            parse_launch_line("dsh web: http://127.0.0.1:45231/?token=xyz").expect("应解析成功");
        assert_eq!(info.base_url.port(), Some(45231));
    }

    #[test]
    fn tolerates_leading_noise_and_trailing_text() {
        let info = parse_launch_line(
            "  [info] ready\nbanner dsh web: http://127.0.0.1:3080/?token=t  (Ctrl-C to stop)",
        )
        .expect("应解析成功");
        assert_eq!(info.token, "t");
    }

    #[test]
    fn rejects_lines_without_token() {
        assert!(parse_launch_line("dsh web: http://127.0.0.1:3080/").is_none());
        assert!(parse_launch_line("starting...").is_none());
        assert!(parse_launch_line("dsh web: http://127.0.0.1:3080/?token=").is_none());
    }
}
