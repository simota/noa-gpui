use std::ffi::{c_char, c_int, c_ulong, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::wake::Waker;

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

// `openpty` is in libutil on Linux (an empty stub since glibc 2.34) and in libSystem on
// macOS.
#[cfg_attr(target_os = "linux", link(name = "util"))]
unsafe extern "C" {
    fn openpty(
        master: *mut c_int,
        slave: *mut c_int,
        name: *mut c_char,
        termios: *const c_void,
        size: *const Winsize,
    ) -> c_int;
    fn setsid() -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
}

#[cfg(target_os = "macos")]
const TIOCSCTTY: c_ulong = 0x2000_7461;
#[cfg(target_os = "macos")]
const TIOCSWINSZ: c_ulong = 0x8008_7467;
#[cfg(target_os = "linux")]
const TIOCSCTTY: c_ulong = 0x540e;
#[cfg(target_os = "linux")]
const TIOCSWINSZ: c_ulong = 0x5414;
const F_SETFD: c_int = 2;
const FD_CLOEXEC: c_int = 1;

const READ_CHUNK_SIZE: usize = 16 * 1024;
// Keep one wake's work bounded so continuous output cannot monopolize the UI.
const READ_BATCH_CHUNKS: usize = 4;

fn winsize(cols: usize, rows: usize) -> Winsize {
    Winsize {
        ws_row: rows.min(u16::MAX as usize) as u16,
        ws_col: cols.min(u16::MAX as usize) as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

fn check(result: c_int) -> io::Result<c_int> {
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

/// A program running in a pseudo-terminal: what it writes arrives on a background
/// thread, which wakes the event loop; [`Pty::read`] takes it on the main thread.
///
/// ```no_run
/// # use ui_platform::{Pty, Waker};
/// # let waker = Waker::default();
/// let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
/// let mut pty = Pty::spawn(std::process::Command::new(shell), 80, 24, waker)?;
/// // In Screen::woken: terminal.feed(cx, &pty.read()), and write the replies back.
/// pty.write(b"ls\r")?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Pty {
    master: File,
    child: Child,
    output: Receiver<Vec<u8>>,
    waker: Waker,
    closed: bool,
}

impl Pty {
    /// Start `command` with a terminal of `cols` × `rows` as its standard input, output
    /// and error, and as its controlling terminal. `TERM` is `xterm-256color` unless
    /// the command sets it. `waker` is woken when output arrives or the program ends.
    pub fn spawn(mut command: Command, cols: usize, rows: usize, waker: Waker) -> io::Result<Self> {
        let (mut master, mut slave) = (-1, -1);
        let size = winsize(cols, rows);
        // SAFETY: the out-pointers are valid; no name buffer or termios is passed.
        check(unsafe {
            openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        })?;
        // SAFETY: `openpty` returned two open descriptors that nothing else owns.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        // The program must not inherit the master side, or it never sees end of file.
        // SAFETY: F_SETFD takes an int argument.
        check(unsafe { fcntl(master.as_raw_fd(), F_SETFD, FD_CLOEXEC) })?;

        if !command.get_envs().any(|(k, _)| k == "TERM") {
            command.env("TERM", "xterm-256color");
        }
        command
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        // SAFETY: runs in the child between fork and exec, calling only async-signal-
        // safe functions: a new session, with the terminal (now standard input) as its
        // controlling terminal.
        unsafe {
            command.pre_exec(|| {
                check(setsid())?;
                check(ioctl(0, TIOCSCTTY, 0 as c_int))?;
                Ok(())
            });
        }
        let master = File::from(master);
        let mut reader = master.try_clone()?;
        let (send, output) = mpsc::channel();
        let mut child = command.spawn()?;
        drop(command);
        let reader_waker = waker.clone();
        let spawned = std::thread::Builder::new()
            .name("pty reader".into())
            .spawn(move || {
                let mut buf = [0u8; READ_CHUNK_SIZE];
                loop {
                    match reader.read(&mut buf) {
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        // End of file, or EIO on Linux once the program has exited.
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if send.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                            reader_waker.wake();
                        }
                    }
                }
                drop(send);
                reader_waker.wake();
            });
        if let Err(error) = spawned {
            // A failed spawn returns no Pty whose Drop could reap the child.
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self {
            master,
            child,
            output,
            waker,
            closed: false,
        })
    }

    /// Available output, up to 64 KiB per call. If the batch fills, the event loop is
    /// woken again to read the rest. Output stays queued in order without dropping
    /// bytes. This call never waits for new output to arrive.
    pub fn read(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..READ_BATCH_CHUNKS {
            match self.output.try_recv() {
                Ok(chunk) => bytes.extend_from_slice(&chunk),
                Err(TryRecvError::Empty) => return bytes,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    return bytes;
                }
            }
        }
        // The producer may already be finished, so do not depend on another write
        // to wake the consumer for the next batch.
        self.waker.wake();
        bytes
    }

    /// Whether the program closed the terminal (it exited) and all its output was read.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Send `bytes` to the program as typed input.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.master.write_all(bytes)
    }

    /// Tell the program the terminal is now `cols` × `rows` (it gets `SIGWINCH`).
    pub fn resize(&self, cols: usize, rows: usize) -> io::Result<()> {
        let size = winsize(cols, rows);
        // SAFETY: TIOCSWINSZ reads a `struct winsize`.
        check(unsafe { ioctl(self.master.as_raw_fd(), TIOCSWINSZ, &size as *const Winsize) })?;
        Ok(())
    }

    /// The program's exit status, once it has exited.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Pty {
    /// End the program if it is still running.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Read until the program closes the terminal (at most five seconds).
    fn read_all(pty: &mut Pty) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut out = Vec::new();
        while !pty.is_closed() && Instant::now() < deadline {
            out.extend(pty.read());
            std::thread::sleep(Duration::from_millis(10));
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn programs_run_on_a_terminal_of_the_given_size() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "stty size; test -t 0 && echo tty; echo $TERM"]);
        let waker = Waker::default();
        let mut pty = Pty::spawn(command, 100, 30, waker.clone()).expect("spawn");
        let out = read_all(&mut pty);
        assert!(out.contains("30 100"), "{out:?}");
        assert!(out.contains("tty"), "{out:?}");
        assert!(out.contains("xterm-256color"), "{out:?}");
        assert!(waker.take(), "output woke the event loop");
    }

    #[test]
    fn input_reaches_the_program_and_resizing_is_seen() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "read line; echo \"got $line\"; stty size"]);
        let mut pty = Pty::spawn(command, 80, 24, Waker::default()).expect("spawn");
        pty.resize(120, 40).expect("resize");
        pty.write(b"hello\r").expect("write");
        let out = read_all(&mut pty);
        assert!(out.contains("got hello"), "{out:?}");
        assert!(out.contains("40 120"), "{out:?}");
        assert!(pty.try_wait().expect("wait").is_some_and(|s| s.success()));
    }

    #[test]
    fn a_burst_of_output_yields_between_reads_without_losing_bytes() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", ":"]);
        let waker = Waker::default();
        let mut pty = Pty::spawn(command, 80, 24, waker.clone()).expect("spawn");
        read_all(&mut pty);
        assert!(pty.is_closed());

        // Queue a finished producer's burst directly, independently of OS pipe
        // capacity and thread scheduling. Each chunk is one PTY read's maximum.
        let (send, output) = mpsc::channel();
        let expected: Vec<_> = (0..8)
            .flat_map(|chunk| std::iter::repeat_n(chunk, 16 * 1024))
            .collect();
        for chunk in expected.chunks(16 * 1024) {
            send.send(chunk.to_vec()).unwrap();
        }
        drop(send);
        pty.output = output;
        pty.closed = false;
        waker.take();

        let mut actual = pty.read();
        assert!(
            actual.len() <= 64 * 1024,
            "a single read must yield to the UI"
        );
        assert!(!pty.is_closed(), "the remaining output is still pending");
        assert!(waker.take(), "buffered output schedules another wake");
        for _ in 0..8 {
            if pty.is_closed() {
                break;
            }
            let next = pty.read();
            assert!(next.len() <= 64 * 1024);
            actual.extend(next);
        }
        assert!(pty.is_closed());
        assert_eq!(actual, expected);
    }

    #[test]
    fn dropping_a_busy_terminal_does_not_wait_for_output_to_be_read() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec yes output"]);
        let waker = Waker::default();
        let pty = Pty::spawn(command, 80, 24, waker.clone()).expect("spawn");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !waker.take() {
            assert!(Instant::now() < deadline, "the program produced no output");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Close with unread output still queued; no consumer should be required.
        let (send, done) = mpsc::channel();
        let dropper = std::thread::spawn(move || {
            drop(pty);
            send.send(()).unwrap();
        });
        done.recv_timeout(Duration::from_secs(5))
            .expect("dropping the terminal must not require a consumer");
        dropper.join().unwrap();
    }
}
