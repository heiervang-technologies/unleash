use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

pub type LocalSubmitHook = Arc<dyn Fn() + Send + Sync>;

pub struct HeadfulInput {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Covers a whole API turn. The terminal input copier takes the same lock
    /// per read, so local keystrokes remain buffered while an API turn owns
    /// the stateful conversation.
    pub turn: Arc<AsyncMutex<()>>,
    killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
}

impl HeadfulInput {
    pub fn terminate(&self) {
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }
}

pub struct HeadfulProcess {
    pub input: Arc<HeadfulInput>,
    exited: Option<oneshot::Receiver<()>>,
    killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    _terminal_mode: Option<TerminalMode>,
}

impl HeadfulProcess {
    pub fn take_exit_receiver(&mut self) -> oneshot::Receiver<()> {
        self.exited
            .take()
            .expect("headful exit receiver already taken")
    }

    pub fn terminate(&self) {
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }
}

pub fn spawn(
    executable: &Path,
    args: &[String],
    environment: &[(String, String)],
    local_submit_hook: Option<LocalSubmitHook>,
) -> io::Result<HeadfulProcess> {
    let size = terminal_size();
    let pair = native_pty_system()
        .openpty(size)
        .map_err(io::Error::other)?;

    let mut command = CommandBuilder::new(executable);
    for arg in args {
        command.arg(arg);
    }
    for (key, value) in environment {
        command.env(key, value);
    }

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(io::Error::other)?;
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
    let writer = pair.master.take_writer().map_err(io::Error::other)?;
    let killer = Arc::new(Mutex::new(child.clone_killer()));
    let writer = Arc::new(Mutex::new(writer));
    let turn = Arc::new(AsyncMutex::new(()));

    std::thread::Builder::new()
        .name("unleash-gateway-pty-output".into())
        .spawn(move || copy_output(reader))
        .map_err(io::Error::other)?;

    let input_writer = Arc::clone(&writer);
    let input_turn = Arc::clone(&turn);
    std::thread::Builder::new()
        .name("unleash-gateway-pty-input".into())
        .spawn(move || copy_input(input_writer, input_turn, local_submit_hook))
        .map_err(io::Error::other)?;

    let (exit_tx, exit_rx) = oneshot::channel();
    std::thread::Builder::new()
        .name("unleash-gateway-child-wait".into())
        .spawn(move || {
            let _ = child.wait();
            let _ = exit_tx.send(());
        })
        .map_err(io::Error::other)?;

    Ok(HeadfulProcess {
        input: Arc::new(HeadfulInput {
            writer,
            turn,
            killer: Arc::clone(&killer),
        }),
        exited: Some(exit_rx),
        killer,
        _terminal_mode: TerminalMode::enter_raw()?,
    })
}

fn copy_output(mut reader: Box<dyn Read + Send>) {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if stdout.write_all(&buffer[..read]).is_err() || stdout.flush().is_err() {
                    break;
                }
            }
        }
    }
}

fn copy_input(
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    turn: Arc<AsyncMutex<()>>,
    local_submit_hook: Option<LocalSubmitHook>,
) {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut buffer = [0u8; 4096];
    loop {
        match stdin.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let _turn_guard = Arc::clone(&turn).blocking_lock_owned();
                if buffer[..read]
                    .iter()
                    .any(|byte| matches!(byte, b'\r' | b'\n'))
                {
                    if let Some(hook) = &local_submit_hook {
                        hook();
                    }
                }
                let Ok(mut writer) = writer.lock() else {
                    break;
                };
                if writer.write_all(&buffer[..read]).is_err() || writer.flush().is_err() {
                    break;
                }
            }
        }
    }
}

fn terminal_size() -> PtySize {
    let mut size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCGWINSZ writes exactly one winsize structure for a valid fd.
    let result = unsafe { libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut size) };
    if result != 0 || size.ws_row == 0 || size.ws_col == 0 {
        return PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };
    }
    PtySize {
        rows: size.ws_row,
        cols: size.ws_col,
        pixel_width: size.ws_xpixel,
        pixel_height: size.ws_ypixel,
    }
}

struct TerminalMode {
    original: libc::termios,
}

impl TerminalMode {
    fn enter_raw() -> io::Result<Option<Self>> {
        if !io::stdin().is_terminal() {
            return Ok(None);
        }
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: tcgetattr initializes the provided termios on success.
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: success above proves initialization.
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        // SAFETY: cfmakeraw mutates a valid termios structure.
        unsafe { libc::cfmakeraw(&mut raw) };
        // Keep output processing enabled so terminal newlines render normally.
        raw.c_oflag |= libc::OPOST;
        // SAFETY: fd is stdin and raw is a valid termios structure.
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(Self { original }))
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        // SAFETY: original came from tcgetattr for stdin.
        let _ = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original) };
    }
}
