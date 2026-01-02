// Copyright (C) 2025 Vladislav Nepogodin
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs};

use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::time::{ClockId, clock_gettime};
use nix::unistd::{Pid, SysconfVar, sysconf};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::runtime::Runtime;
use tokio::time::{self, Duration, Instant};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::LinesStream;

const PROC_PATH: &str = "/proc";
const TRUNC_MESSAGE: &str = "\n\n\nTRUNCATED DUE TO TIMEOUT\n\n";

pub type ChildrenCheckValidator = fn(&str) -> bool;

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecStatus {
    pub exit_code: i32,
    pub output: String,
}

pub fn exec_cmd(
    bin: &str,
    args: &[String],
    work_dir: Option<String>,
    is_stderr: bool,
) -> Result<ExecStatus> {
    // fallback to current dir in case nullopt was provided
    let work_dir = if work_dir.is_none() {
        let current_dir = env::current_dir().context("failed to get CWD")?;
        current_dir.as_path().to_str().unwrap().to_owned()
    } else {
        work_dir.unwrap()
    };

    let mut child = Command::new(bin);
    child.args(args).current_dir(work_dir).stdin(Stdio::null());

    // capture only wanted IO
    let child = if is_stderr {
        child.stdout(Stdio::null()).stderr(Stdio::piped())
    } else {
        child.stdout(Stdio::piped()).stderr(Stdio::null())
    }
    .spawn()
    .context("failed to spawn subprocess")?;

    let child_status = child.wait_with_output().context("failed to wait on child")?;
    let exit_code = child_status.status.code().unwrap_or(255);

    let output = String::from_utf8_lossy(if is_stderr {
        &child_status.stderr
    } else {
        &child_status.stdout
    })
    .to_string();

    Ok(ExecStatus { exit_code, output })
}

pub fn exec_proc(
    bin: &str,
    args: &[String],
    start_dir: &str,
    log: &mut Vec<u8>,
    timeout: Option<Duration>,
    validator: Option<ChildrenCheckValidator>,
) -> Result<bool> {
    let rt = Runtime::new().context("Failed to initialize tokio runtime")?;
    let res = rt.block_on(async move {
        return exec_proc_async_ext(bin, args, start_dir, log, timeout, validator).await;
    })?;
    Ok(res)
}

fn kill_proc(pid: u32, signal: Signal) -> Result<()> {
    // convert 1234 to -1234 to kill grand-children too (requires process group 0)
    // let pid = -(pid as i32);
    signal::kill(Pid::from_raw(pid as i32), signal).context("failed to send signal")?;
    Ok(())
}

fn kill_procs(pids: &[Pid], signal: Signal) -> Result<()> {
    for pid in pids {
        kill_proc(pid.as_raw() as u32, signal).context("failed to kill proc")?;
    }
    Ok(())
}

/// Checks if a /proc directory entry path corresponds to a valid process ID.
/// A valid process ID entry is a directory whose name consists of digits
/// and is not "0".
fn is_valid_proc_entry(entry_path: &Path) -> bool {
    entry_path.file_name().and_then(|x| x.to_str()).is_some_and(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) && name != "0"
    })
}

/// Get parts after tcomm from /proc/[pid]/stat.
fn get_proc_parts(stat_content: &str) -> Result<Vec<&str>> {
    let idx_first_paren = stat_content.find('(').context("could not find '(' in stat file")?;
    let idx_last_paren = stat_content.rfind(')').context("could not find ')' in stat file")?;

    // Ensure valid `(tcomm)` structure
    if idx_last_paren <= idx_first_paren {
        anyhow::bail!("Invalid format for (tcomm) in stat file");
    }

    // Extract the string of fields that appear *after* the `(tcomm)` part.
    let fields_after_comm_str = stat_content
        .get((idx_last_paren + 1)..)
        .context("stat file content too short after (tcomm)")?
        .trim_start();

    Ok(fields_after_comm_str.split_whitespace().collect())
}

/// Reads the Parent Process ID (PPID) from /proc/[pid]/stat.
/// Returns -1 on failure to parse or find.
fn get_proc_ppid(pid: Pid) -> Pid {
    let stat_path = format!("{PROC_PATH}/{pid}/stat");

    let stat_content = match fs::read_to_string(stat_path) {
        Ok(content) => content,
        Err(_) => return Pid::from_raw(-1),
    };

    let parts = get_proc_parts(&stat_content);
    // simply return invalid pid
    if parts.is_err() {
        return Pid::from_raw(-1);
    }
    let parts = parts.unwrap();

    // In /proc/[pid]/stat (see https://www.kernel.org/doc/html/latest/filesystems/proc.html)
    // Field 3 is 'state'. Field 4 is 'ppid'.
    const PPID_FIELD_INDEX_IN_PARTS_AFTER_TCOMM: usize = 4 - 3;

    if parts.len() > PPID_FIELD_INDEX_IN_PARTS_AFTER_TCOMM {
        let ppid_str = parts[PPID_FIELD_INDEX_IN_PARTS_AFTER_TCOMM];
        if ppid_str.chars().all(|c| c.is_ascii_digit()) {
            Pid::from_raw(ppid_str.parse::<i32>().unwrap_or(-1))
        } else {
            // PPID string contains non-digits
            Pid::from_raw(-1)
        }
    } else {
        // Not enough parts in stat line
        Pid::from_raw(-1)
    }
}

/// Extracts the runtime of a process in seconds.
///
/// The runtime is calculated as:
/// `system_uptime - (process_starttime_ticks / CLK_TCK)`
fn get_proc_runtime_secs(pid: Pid) -> Result<u64> {
    // 1. Get CLK_TCK (clock ticks per second)
    let clk_tck = match sysconf(SysconfVar::CLK_TCK)? {
        Some(val) if val > 0 => val as u64,
        // fallback to 100
        Some(_) | None => 100,
    };

    // 2. Get system uptime using CLOCK_BOOTTIME for accuracy
    let boottime_spec =
        clock_gettime(ClockId::CLOCK_BOOTTIME).context("failed to get CLOCK_BOOTTIME")?;
    let system_uptime_seconds =
        boottime_spec.tv_sec() as f64 + (boottime_spec.tv_nsec() as f64 / 1_000_000_000.0);

    // 3. Read stat file
    let stat_path = format!("{PROC_PATH}/{}/stat", pid.as_raw());
    let stat_content =
        fs::read_to_string(&stat_path).with_context(|| format!("Failed to read {stat_path}"))?;

    // 4. Parse start_time (field 22) from stat_content
    let parts = get_proc_parts(&stat_content)?;

    // In /proc/[pid]/stat (see https://www.kernel.org/doc/html/latest/filesystems/proc.html)
    // Field 3 is 'state'. Field 22 is 'start_time'.
    const STARTTIME_FIELD_INDEX_IN_PARTS_AFTER_TCOMM: usize = 22 - 3;

    if parts.len() <= STARTTIME_FIELD_INDEX_IN_PARTS_AFTER_TCOMM {
        anyhow::bail!(
            "Not enough fields after (comm) in stat file {} to find starttime (field 22). Found \
             {} fields, need at least {}",
            stat_path,
            parts.len(),
            STARTTIME_FIELD_INDEX_IN_PARTS_AFTER_TCOMM + 1,
        );
    }

    let starttime_ticks_str = parts[STARTTIME_FIELD_INDEX_IN_PARTS_AFTER_TCOMM];
    let starttime_ticks: u64 = starttime_ticks_str.parse::<u64>().with_context(|| {
        format!(
            "Failed to parse starttime ticks '{starttime_ticks_str}' from {stat_path}. Full \
             fields after comm: {parts:?}",
        )
    })?;

    // 5. Calculate runtime starttime_ticks is the time the process started
    // after system boot, in clock ticks.
    let proc_start_time_after_boot_seconds = starttime_ticks as f64 / clk_tck as f64;

    let runtime_seconds_float = system_uptime_seconds - proc_start_time_after_boot_seconds;
    if runtime_seconds_float < 0.0 { Ok(0) } else { Ok(runtime_seconds_float.round() as u64) }
}

/// Gets a list of direct children for a given parent PID.
/// Returns an empty vector on error or if no children are found.
fn get_children_of_pid(parent_pid: Pid) -> Vec<Pid> {
    let mut children = Vec::new();

    if let Ok(entries) = fs::read_dir(PROC_PATH) {
        for entry_result in entries {
            if let Ok(path) = entry_result.map(|x| x.path()) {
                if !path.is_dir() || !is_valid_proc_entry(&path) {
                    continue;
                }

                if let Some(pid) =
                    path.file_name().and_then(|x| x.to_str()).and_then(|x| x.parse::<i32>().ok())
                {
                    let pid = Pid::from_raw(pid);
                    if get_proc_ppid(pid) != parent_pid {
                        continue;
                    }
                    children.push(pid);
                }
            }
        }
    }
    children
}

/// Recursively gets all children (direct and indirect) of a given parent PID.
/// Returns an empty vector if no children or on error.
fn get_children_of_pid_rec(parent_pid: Pid) -> Vec<Pid> {
    let direct_children = get_children_of_pid(parent_pid);
    if direct_children.is_empty() {
        return Vec::new();
    }

    // Start with direct children
    let mut all_children = direct_children.clone();
    for child_pid in direct_children {
        // Iterate over a copy or by reference
        let mut nested_children = get_children_of_pid_rec(child_pid);
        all_children.append(&mut nested_children);
    }

    all_children
}

/// Reads the command line arguments from /proc/[pid]/cmdline.
/// Arguments are null-separated.
/// Returns an empty vector on failure.
fn get_cmdline_from_pid(pid: Pid) -> Vec<String> {
    let cmdline_path = format!("{PROC_PATH}/{pid}/cmdline");
    if let Ok(bytes) = fs::read(cmdline_path) {
        if bytes.is_empty() {
            return Vec::new();
        }
        // Arguments are separated by null bytes. The last argument might also be
        // null-terminated. `split` by b'\0'` will produce an empty slice after the
        // last null if it's truly null-terminated.
        bytes
            .split(|&b| b == 0u8)
            .filter(|arg_bytes| !arg_bytes.is_empty())
            .map(|arg_bytes| String::from_utf8_lossy(arg_bytes).into_owned())
            .collect()
    } else {
        Vec::new()
    }
}

fn proc_children_checker(
    ppid: Pid,
    timeout: Duration,
    validator: ChildrenCheckValidator,
) -> Result<()> {
    // query for possible test cases in the children of executed proc,
    // instead of trying to kill main proc
    let children = get_children_of_pid_rec(ppid);

    let filtered_children: Vec<_> = children
        .into_iter()
        .filter(|&pid| {
            // skip child if it ran less than timeout
            if let Ok(child_runtime) = get_proc_runtime_secs(pid)
                && Duration::from_secs(child_runtime) < timeout
            {
                return false;
            }
            let cmdline = get_cmdline_from_pid(pid);
            // warn!("cmdline expired going to cancel: {cmdline:?}");

            // check whole cmdline instead of individual args of the proc
            let cmdline = cmdline.join(" ");
            validator(&cmdline)
        })
        .collect();
    kill_procs(&filtered_children, Signal::SIGTERM).context("failed to kill children")?;

    Ok(())
}

pub async fn exec_proc_async_ext<W>(
    bin: &str,
    args: &[String],
    start_dir: &str,
    mut log: W,
    timeout: Option<Duration>,
    children_val: Option<ChildrenCheckValidator>,
) -> Result<bool>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut cmd = tokio::process::Command::new(bin)
        .args(args)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(start_dir)
        //.process_group(0)
        .spawn()
        .context("failed to spawn subprocess")?;

    // Take ownership of stdout and stderr from child.
    let child_stdout = cmd.stdout.take().unwrap();
    let child_stderr = cmd.stderr.take().unwrap();

    // Wrap them up and merge them.
    let stdout = LinesStream::new(BufReader::new(child_stdout).lines());
    let stderr = LinesStream::new(BufReader::new(child_stderr).lines());
    let mut merged = StreamExt::merge(stdout, stderr);

    // in case timeout is not set, set it to 12h
    let timeout = timeout.unwrap_or(Duration::from_secs(12 * 60 * 60));

    // Sleep for the time of timeout
    let sleep = time::sleep(timeout);
    tokio::pin!(sleep);

    // Read child IO, and wait for timeout cancelation to kill the child
    loop {
        tokio::select! {
            // Iterate through the stream line-by-line asynchronously
            line = merged.next() => {
                match line {
                    // EOF
                    None => break,
                    Some(line_res) => {
                        // Since reading a line may fail, we use the question-mark to unwrap the line.
                        let line_str = line_res.context("failed to read line from subprocess")?;
                        let formatted_line = format!("{line_str}\n");

                        // Write asynchronously to the provided writer
                        log.write_all(formatted_line.as_bytes()).await.context("failed to write log")?;
                    }
                }
            }
            () = &mut sleep => {
                if let Some(pid) = cmd.id() {
                    if let Some(validator) = children_val {
                        // reset timer
                        sleep.as_mut().reset(Instant::now() + timeout);

                        // check if children match test case, and kill if they do
                        proc_children_checker(Pid::from_raw(pid as i32), timeout, validator).context("failed to check children")?;
                        continue;
                    }
                    kill_proc(pid, Signal::SIGTERM).context("failed to kill proc")?;

                    // write all-in  and flush
                    log.write_all(TRUNC_MESSAGE.as_bytes()).await.context("failed to write truncation log")?;
                    log.flush().await.context("failed to flush log")?;
                    break;
                }
            }
            res = cmd.wait() => {
                let status = res?;
                log.flush().await?;
                return Ok(status.success());
            }
        }
    }

    let status = cmd.wait().await?;
    log.flush().await?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::{Cursor, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;
    use tempfile::TempDir;

    async fn script(script: &str, timeout: Option<Duration>) -> Result<(bool, String, Duration)> {
        let mut output = Vec::new();
        let start = Instant::now();

        // FIXME: wrap buffer so it implements AsyncWrite
        let mut buf_cursor = Cursor::new(&mut output);

        let success = exec_proc_async_ext(
            "sh",
            &["-c".into(), script.into()],
            "/tmp",
            &mut buf_cursor,
            timeout,
            None,
        )
        .await?;

        let duration = start.elapsed();
        let output = String::from_utf8_lossy(&output).into_owned();
        Ok((success, output, duration))
    }

    fn create_named_script(temp_dir: &Path, script_name: &str, script_content: &str) -> PathBuf {
        let script_path = temp_dir.join(script_name);
        let mut file = File::create(&script_path).expect("failed to create script file");
        writeln!(file, "#!/bin/sh\n").unwrap();
        write!(file, "{script_content}").unwrap();
        drop(file);

        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .expect("failed to set script executable");
        script_path
    }

    async fn exec_with_validator(
        command: &str,
        args: &[String],
        work_dir: &Path,
        log_output: &mut Vec<u8>,
        timeout: Option<Duration>,
        validator: Option<ChildrenCheckValidator>,
    ) -> Result<(bool, Duration)> {
        let start_time = Instant::now();

        // FIXME: wrap buffer so it implements AsyncWrite
        let mut buf_cursor = Cursor::new(log_output);

        let success_status = exec_proc_async_ext(
            command,
            args,
            work_dir.to_str().unwrap(),
            &mut buf_cursor,
            timeout,
            validator,
        )
        .await?;
        let elapsed_time = start_time.elapsed();
        Ok((success_status, elapsed_time))
    }

    async fn spawn_children() -> Result<(tokio::process::Child, Pid)> {
        let child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 5 & sleep 6 & wait")
            .spawn()
            .unwrap();

        let pid = Pid::from_raw(child.id().unwrap() as i32);
        time::sleep(Duration::from_millis(300)).await;
        Ok((child, pid))
    }

    #[tokio::test]
    async fn hello_world() {
        let (success, output, _) = script("/bin/echo 'hello world'", None).await.unwrap();
        assert!(success);
        assert_eq!(output, "hello world\n");
    }

    #[tokio::test]
    async fn timeout() {
        let (success, output, _) = script(
            "for x in `seq 100`; do
        /bin/echo AAAAAAAAAAAAAAAAAAAAAAAA
        >&2 /bin/echo BBBB
        sleep 1
    done",
            Some(Duration::from_secs(1)),
        )
        .await
        .unwrap();
        assert!(!success);
        assert!(output.contains("AAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(output.contains("TRUNCATED DUE TO TIMEOUT"));
    }

    #[tokio::test]
    async fn get_children_rec_pids() {
        let (_, ppid) = spawn_children().await.unwrap();

        let children = get_children_of_pid_rec(ppid);
        assert_eq!(children.len(), 2);
        let cmdlines = children.iter().map(|pid| get_cmdline_from_pid(*pid)).collect::<Vec<_>>();
        assert_eq!(cmdlines, vec![vec!["sleep", "5"], vec!["sleep", "6"]]);

        kill_procs(&children, Signal::SIGTERM).expect("failed to kill children");
        kill_proc(ppid.as_raw() as u32, Signal::SIGTERM).expect("failed to kill proc");
    }

    #[tokio::test]
    async fn get_children_pids() {
        let (_, ppid) = spawn_children().await.unwrap();

        let children = get_children_of_pid(ppid);
        assert_eq!(children.len(), 2);
        let cmdlines = children.iter().map(|pid| get_cmdline_from_pid(*pid)).collect::<Vec<_>>();
        assert_eq!(cmdlines, vec![vec!["sleep", "5"], vec!["sleep", "6"]]);

        kill_procs(&children, Signal::SIGTERM).expect("failed to kill children");
        kill_proc(ppid.as_raw() as u32, Signal::SIGTERM).expect("failed to kill proc");
    }

    #[tokio::test]
    async fn get_ppid() {
        let (_, ppid) = spawn_children().await.unwrap();

        let children = get_children_of_pid(ppid);
        assert_eq!(children.len(), 2);
        assert_eq!(get_proc_ppid(children[0]), ppid);

        kill_procs(&children, Signal::SIGTERM).expect("failed to kill children");
        kill_proc(ppid.as_raw() as u32, Signal::SIGTERM).expect("failed to kill proc");
    }

    #[tokio::test]
    async fn test_get_proc_runtime_basic() -> Result<()> {
        let child = tokio::process::Command::new("sleep").arg("3").spawn()?;
        let pid = Pid::from_raw(child.id().unwrap() as i32);

        time::sleep(Duration::from_secs(1)).await;
        let runtime1 = get_proc_runtime_secs(pid).expect("failed runtime check 1");
        assert!(runtime1 <= 1, "Initial runtime {runtime1}s is too high, expected <= 1s");

        time::sleep(Duration::from_secs(2)).await;
        let runtime2 = get_proc_runtime_secs(pid).expect("failed runtime check 2");
        // After sleeping for 2 more seconds, runtime should be initial_runtime + 2s.
        // If runtime1 was (0.1s rounded to) 0s, now it's (2.1s rounded to) 2s.
        // If runtime1 was (0.6s rounded to) 1s, now it's (2.6s rounded to) 3s.
        assert!(
            (runtime2 == runtime1 + 2 || runtime2 == runtime1 + 1 || runtime2 == runtime1 + 3),
            "Runtime after 2s ({runtime2}) not consistent with initial runtime ({runtime1}). \
             Expected ~2s increase."
        );

        kill_proc(pid.as_raw() as u32, Signal::SIGTERM).expect("failed to kill proc");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_proc_runtime_for_self_after_delay() -> Result<()> {
        // Get PID of the current test process
        let own_pid = Pid::this();

        // Allow a tiny bit of time for the test process to be "running" according to /proc
        time::sleep(Duration::from_millis(50)).await;
        let initial_runtime =
            get_proc_runtime_secs(own_pid).expect("failed to get initial self runtime");

        time::sleep(Duration::from_secs(1)).await;

        let runtime_after_delay =
            get_proc_runtime_secs(own_pid).expect("failed to get self runtime after delay");

        let diff = runtime_after_delay.saturating_sub(initial_runtime);
        // Expect difference to be 1s, allow for rounding (0, 1, or 2)
        assert!(
            (diff == 1 || diff == 0 || diff == 2),
            "Runtime difference for self expected around 1s, got {diff}s (initial: \
             {initial_runtime}s, after delay: {runtime_after_delay}s)"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_proc_runtime_for_non_existent_pid() {
        let non_existent_pid = Pid::from_raw(i32::MAX - 100);
        let result = get_proc_runtime_secs(non_existent_pid);
        assert!(result.is_err());
        if let Err(e) = result {
            let err_string = e.to_string();
            assert!(
                err_string.contains("No such file or directory")
                    || err_string.contains("Failed to read"),
                "Error message '{err_string}' not as expected for non-existent PID."
            );
        }
    }

    #[tokio::test]
    async fn ext_validator_kills_child_main_succeeds() {
        let temp_dir = TempDir::new().unwrap();
        let child_script_name = "test_kill_suc.sh";
        let child_script_content =
            "echo 'Child to be killed starting'; sleep 10; echo 'Child survived?'";
        create_named_script(temp_dir.path(), child_script_name, child_script_content);

        let main_script_content = format!(
            "./{child_script_name} &\n echo 'Main: child launched' \n sleep 2 \n echo 'Main: task \
             done' \n wait",
        );

        let validator: ChildrenCheckValidator =
            |cmdline: &str| -> bool { cmdline.contains("sleep") };

        let mut log = Vec::new();
        let (success, duration) = exec_with_validator(
            "sh",
            &["-c".to_string(), main_script_content.to_string()],
            temp_dir.path(),
            &mut log,
            Some(Duration::from_secs(1)),
            Some(validator),
        )
        .await
        .unwrap();

        let output = String::from_utf8_lossy(&log);

        assert!(success);
        assert!(!output.contains(TRUNC_MESSAGE));

        assert!(duration > Duration::from_secs(1) && duration < Duration::from_secs(4));
    }

    #[tokio::test]
    async fn ext_validator_ignores_child_if_runtime_too_short() {
        let temp_dir = TempDir::new().unwrap();
        let child_script_name = "quick_child.sh";
        let child_script_content =
            "echo 'Quick child starting'; sleep 0.2; echo 'Quick child finished'";
        create_named_script(temp_dir.path(), child_script_name, child_script_content);

        let main_script_content = format!(
            "./{child_script_name} &\n echo 'Main: quick child launched' \n sleep 2 \n echo \
             'Main: task done' \n wait",
        );

        let validator: ChildrenCheckValidator =
            |cmdline: &str| -> bool { cmdline.contains("quick_child.sh") };

        let mut log = Vec::new();
        let (success, duration) = exec_with_validator(
            "sh",
            &["-c".to_string(), main_script_content.to_string()],
            temp_dir.path(),
            &mut log,
            Some(Duration::from_secs(1)),
            Some(validator),
        )
        .await
        .unwrap();

        let output = String::from_utf8_lossy(&log);

        assert!(success);
        assert!(output.contains("Quick child starting"));
        assert!(output.contains("Quick child finished"));
        assert!(output.contains("Main: task done"));
        assert!(!output.contains(TRUNC_MESSAGE));

        assert!(duration > Duration::from_secs(1) && duration < Duration::from_secs(4));
    }
}
