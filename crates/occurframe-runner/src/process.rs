use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use occurframe_conformance::{normalize_completed, normalize_execution_failure};
use occurframe_wire::{
    CaseMessage, Diagnostic, ExecutionStatus, HelloMessage, RUNNER_PROTOCOL_VERSION, RunnerMessage,
    StartedMessage, Vector,
};

use crate::{
    CaseExecution, ProtocolSchema, RunnerBuild, RunnerDiagnostic,
    batch::execution,
    diagnostics::{BoundedTail, capture_stderr},
};

/// Default startup/hello/pre-`started` watchdog. It is operational configuration,
/// not recurrence semantics.
pub const DEFAULT_INFRASTRUCTURE_WATCHDOG: Duration = Duration::from_secs(30);
const MAX_PROTOCOL_LINE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
enum StdoutEvent {
    Line(String),
    Eof,
    Io(String),
}

struct RunningProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_events: Receiver<StdoutEvent>,
    stderr_tail: Arc<Mutex<BoundedTail>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl RunningProcess {
    fn spawn(build: &RunnerBuild, root: &Path, stderr_capacity: usize) -> std::io::Result<Self> {
        let program_path = Path::new(&build.launch.program);
        let program = if program_path.components().count() > 1 && program_path.is_relative() {
            root.join(program_path)
        } else {
            program_path.to_path_buf()
        };
        #[cfg(windows)]
        let program = {
            let mut platform_program = program;
            if !platform_program.exists() && platform_program.extension().is_none() {
                platform_program.set_extension("exe");
            }
            platform_program
        };
        let working_directory = root.join(&build.launch.working_directory);
        let mut command = Command::new(program);
        command
            .args(&build.launch.arguments)
            .current_dir(working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(&build.launch.environment);
        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let (sender, receiver) = mpsc::channel();
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = sender.send(StdoutEvent::Eof);
                        break;
                    }
                    Ok(_) if line.len() > MAX_PROTOCOL_LINE_BYTES => {
                        let _ = sender.send(StdoutEvent::Io(format!(
                            "protocol line exceeds {MAX_PROTOCOL_LINE_BYTES} bytes"
                        )));
                        break;
                    }
                    Ok(_) if line.trim().is_empty() => {}
                    Ok(_) => {
                        if sender.send(StdoutEvent::Line(line)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(StdoutEvent::Io(error.to_string()));
                        break;
                    }
                }
            }
        });
        let (stderr_tail, stderr_thread) = capture_stderr(stderr, stderr_capacity);
        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout_events: receiver,
            stderr_tail,
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
        })
    }

    fn write_message(&mut self, message: &RunnerMessage) -> std::io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "runner stdin closed")
        })?;
        serde_json::to_writer(&mut *stdin, message)?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }

    fn receive(&self, timeout: Duration) -> std::result::Result<String, ReceiveFailure> {
        match self.stdout_events.recv_timeout(timeout) {
            Ok(StdoutEvent::Line(line)) => Ok(line),
            Ok(StdoutEvent::Eof) | Err(RecvTimeoutError::Disconnected) => {
                Err(ReceiveFailure::ProcessExit)
            }
            Ok(StdoutEvent::Io(error)) => Err(ReceiveFailure::Io(error)),
            Err(RecvTimeoutError::Timeout) => Err(ReceiveFailure::Watchdog),
        }
    }

    fn stderr_tail(&self) -> Option<String> {
        self.stderr_tail.lock().ok().and_then(|tail| {
            let text = tail.text();
            (!text.is_empty()).then_some(text)
        })
    }

    fn terminate(&mut self) {
        self.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RunningProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[derive(Debug)]
enum ReceiveFailure {
    Watchdog,
    ProcessExit,
    Io(String),
}

/// Stateful supervisor for one configured engine process. A timed-out or failed
/// process is always discarded and recreated before a later case.
pub struct RunnerSupervisor {
    build: RunnerBuild,
    repository_root: PathBuf,
    schema: ProtocolSchema,
    infrastructure_watchdog: Duration,
    stderr_capacity: usize,
    process: Option<RunningProcess>,
    hello: Option<HelloMessage>,
}

impl RunnerSupervisor {
    #[must_use]
    pub fn new(
        build: RunnerBuild,
        repository_root: PathBuf,
        schema: ProtocolSchema,
        infrastructure_watchdog: Duration,
        stderr_capacity: usize,
    ) -> Self {
        Self {
            build,
            repository_root,
            schema,
            infrastructure_watchdog,
            stderr_capacity,
            process: None,
            hello: None,
        }
    }

    /// Execute one already-validated corpus vector.
    pub fn execute(&mut self, vector: &Vector, sequence: usize) -> CaseExecution {
        if let Err((code, message)) = self.ensure_process() {
            return self.failure(vector, ExecutionStatus::RunnerFailure, &code, &message);
        }
        let request_id = format!("case-{sequence:06}-{}", vector.id);
        let case = RunnerMessage::Case(CaseMessage {
            protocol_version: RUNNER_PROTOCOL_VERSION.into(),
            request_id: request_id.clone(),
            vector: vector.clone(),
            budget_ms: occurframe_wire::OFFICIAL_BUDGET_MS,
        });
        if let Err(error) = self
            .process
            .as_mut()
            .expect("process established")
            .write_message(&case)
        {
            return self.fail_and_discard(
                vector,
                ExecutionStatus::RunnerFailure,
                "case_write_failure",
                &error.to_string(),
            );
        }

        match self.receive_protocol(self.infrastructure_watchdog) {
            Ok(RunnerMessage::Started(StartedMessage {
                protocol_version,
                request_id: observed_id,
            })) if protocol_version == RUNNER_PROTOCOL_VERSION && observed_id == request_id => {}
            Ok(_) => {
                return self.fail_and_discard(
                    vector,
                    ExecutionStatus::RunnerFailure,
                    "missing_started",
                    "runner did not emit the attributable started acknowledgement",
                );
            }
            Err(ReceiveFailure::Watchdog) => {
                return self.fail_and_discard(
                    vector,
                    ExecutionStatus::RunnerFailure,
                    "infrastructure_watchdog",
                    "runner did not acknowledge the case before the infrastructure watchdog expired",
                );
            }
            Err(error) => {
                return self.fail_and_discard(
                    vector,
                    ExecutionStatus::RunnerFailure,
                    "pre_started_failure",
                    &format_receive_failure(&error),
                );
            }
        }

        let engine_budget = Duration::from_millis(occurframe_wire::OFFICIAL_BUDGET_MS);
        match self.receive_protocol(engine_budget) {
            Ok(RunnerMessage::Result(result))
                if result.protocol_version == RUNNER_PROTOCOL_VERSION
                    && result.request_id == request_id =>
            {
                let hello = self.hello.as_ref().expect("validated hello");
                let observation =
                    normalize_completed(&vector.corpus_version, &vector.id, hello, result);
                execution(self.build.build_id.clone(), vector, observation, None)
            }
            Ok(_) => self.fail_and_discard(
                vector,
                ExecutionStatus::RunnerFailure,
                "invalid_terminal_message",
                "runner did not emit exactly one attributable result after started",
            ),
            Err(ReceiveFailure::Watchdog) => self.fail_and_discard(
                vector,
                ExecutionStatus::Timeout,
                "engine_timeout",
                "no result was observed before the engine budget expired",
            ),
            Err(error) => self.fail_and_discard(
                vector,
                ExecutionStatus::RunnerFailure,
                "post_started_process_failure",
                &format_receive_failure(&error),
            ),
        }
    }

    fn ensure_process(&mut self) -> std::result::Result<(), (String, String)> {
        if self.process.is_some() {
            return Ok(());
        }
        let process =
            RunningProcess::spawn(&self.build, &self.repository_root, self.stderr_capacity)
                .map_err(|error| ("startup_failure".into(), error.to_string()))?;
        self.process = Some(process);
        let message = match self.receive_protocol(self.infrastructure_watchdog) {
            Ok(message) => message,
            Err(error) => {
                let diagnostic = format_receive_failure(&error);
                self.discard_process();
                return Err(("hello_failure".into(), diagnostic));
            }
        };
        let RunnerMessage::Hello(hello) = message else {
            self.discard_process();
            return Err((
                "malformed_hello".into(),
                "first protocol message was not hello".into(),
            ));
        };
        if let Err(error) = self.build.validate_hello(&hello) {
            self.discard_process();
            return Err(("identity_mismatch".into(), error));
        }
        self.hello = Some(hello);
        Ok(())
    }

    fn receive_protocol(
        &self,
        timeout: Duration,
    ) -> std::result::Result<RunnerMessage, ReceiveFailure> {
        let line = self
            .process
            .as_ref()
            .expect("process established")
            .receive(timeout)?;
        let value: serde_json::Value = serde_json::from_str(&line)
            .map_err(|error| ReceiveFailure::Io(format!("malformed protocol JSON: {error}")))?;
        self.schema
            .validate(&value)
            .map_err(|error| ReceiveFailure::Io(format!("protocol schema violation: {error}")))?;
        serde_json::from_value(value)
            .map_err(|error| ReceiveFailure::Io(format!("invalid protocol message: {error}")))
    }

    fn fail_and_discard(
        &mut self,
        vector: &Vector,
        status: ExecutionStatus,
        code: &str,
        message: &str,
    ) -> CaseExecution {
        let hello = self.hello.clone();
        let stderr_tail = self.process.take().and_then(|mut process| {
            process.terminate();
            process.stderr_tail()
        });
        self.hello = None;
        self.failure_with_hello(vector, status, code, message, stderr_tail, hello.as_ref())
    }

    fn failure(
        &self,
        vector: &Vector,
        status: ExecutionStatus,
        code: &str,
        message: &str,
    ) -> CaseExecution {
        self.failure_with_hello(vector, status, code, message, None, self.hello.as_ref())
    }

    fn failure_with_hello(
        &self,
        vector: &Vector,
        status: ExecutionStatus,
        code: &str,
        message: &str,
        stderr_tail: Option<String>,
        observed_hello: Option<&HelloMessage>,
    ) -> CaseExecution {
        let fallback;
        let hello = if let Some(hello) = observed_hello {
            hello
        } else {
            fallback = self.build.fallback_hello();
            &fallback
        };
        let observation =
            normalize_execution_failure(&vector.corpus_version, &vector.id, hello, status);
        execution(
            self.build.build_id.clone(),
            vector,
            observation,
            Some(RunnerDiagnostic {
                diagnostic: Diagnostic {
                    code: code.into(),
                    message: message.into(),
                    details: None,
                },
                stderr_tail,
            }),
        )
    }

    fn discard_process(&mut self) {
        if let Some(mut process) = self.process.take() {
            process.terminate();
        }
        self.hello = None;
    }
}

fn format_receive_failure(error: &ReceiveFailure) -> String {
    match error {
        ReceiveFailure::Watchdog => "watchdog expired".into(),
        ReceiveFailure::ProcessExit => "runner process exited before a valid message".into(),
        ReceiveFailure::Io(message) => message.clone(),
    }
}

impl Drop for RunnerSupervisor {
    fn drop(&mut self) {
        self.discard_process();
    }
}
