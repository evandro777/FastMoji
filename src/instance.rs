//! Single instance: the first process listens on a Unix socket, later ones send it a command.
use std::{
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_COMMAND: u64 = 64;

/// None without `XDG_RUNTIME_DIR`: a shared directory such as `/tmp` would let other users block the
/// socket or send commands to it.
fn socket_path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    // Every `flatpak run` gets its own sandbox, and only this per-app directory is shared
    // between them (the rest of the runtime dir is a private tmpfs).
    Some(match std::env::var("FLATPAK_ID") {
        Ok(app) => dir.join("app").join(app).join("fastmoji.sock"),
        Err(_) => dir.join("fastmoji.sock"),
    })
}

/// Send `command` to a running instance. Returns false if none is listening.
pub fn send(command: &str) -> bool {
    socket_path().is_some_and(|path| send_to(&path, command))
}

fn send_to(path: &Path, command: &str) -> bool {
    match UnixStream::connect(path) {
        Ok(mut stream) => stream.write_all(format!("{command}\n").as_bytes()).is_ok(),
        Err(_) => false,
    }
}

/// Become the running instance. `on_command` is called on a background thread for each command.
/// Fails with `AddrInUse` when another instance is already listening (two starts can race past
/// `send`), so the caller must forward its command and exit instead of taking the socket over.
pub fn listen(on_command: impl Fn(String) + Send + 'static) -> std::io::Result<()> {
    let path = socket_path().ok_or_else(|| std::io::Error::other("XDG_RUNTIME_DIR is not set"))?;
    listen_at(&path, on_command)
}

fn listen_at(path: &Path, on_command: impl Fn(String) + Send + 'static) -> std::io::Result<()> {
    // Held until the bind is settled: without it, a second start could delete the socket a first one just bound.
    let lock = std::fs::File::create(path.with_extension("lock"))?;
    lock.lock()?;
    let listener = match UnixListener::bind(path) {
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Err(e);
            }
            // Nobody answers: stale socket from a crashed run. If another start removes and binds
            // it first, this bind fails with AddrInUse too, and the caller forwards as above.
            let _ = std::fs::remove_file(path);
            UnixListener::bind(path)?
        }
        other => other?,
    };
    drop(lock);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // One slow or silent client must not block the others (a hung `--toggle` is useless);
            // commands are a few bytes, so cap the length too.
            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
            let mut line = String::new();
            if BufReader::new(stream.take(MAX_COMMAND)).read_line(&mut line).is_ok() {
                on_command(line.trim().to_string());
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn temp_socket(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fastmoji-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("fastmoji.sock")
    }

    #[test]
    fn second_listener_is_refused_and_commands_reach_the_first() {
        let path = temp_socket("busy");
        let (tx, rx) = mpsc::channel();
        // The second start's liveness probe connects without sending anything: an empty command, ignored by the app.
        listen_at(&path, move |c| _ = (!c.is_empty()).then(|| tx.send(c))).unwrap();
        let second = listen_at(&path, |_| {});
        assert_eq!(second.unwrap_err().kind(), ErrorKind::AddrInUse);
        assert!(send_to(&path, "toggle"));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), "toggle");
    }

    #[test]
    fn simultaneous_starts_after_a_crash_leave_one_live_instance() {
        let path = temp_socket("race");
        drop(UnixListener::bind(&path).unwrap()); // stale file
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let starts: Vec<_> = (0..8)
            .map(|_| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    listen_at(&path, |_| {})
                })
            })
            .collect();
        let results: Vec<_> = starts.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
        assert!(
            results.iter().flatten().count() == 1 && send_to(&path, "show"),
            "the winner must stay reachable"
        );
    }

    #[test]
    fn stale_socket_file_is_replaced() {
        let path = temp_socket("stale");
        drop(UnixListener::bind(&path).unwrap()); // a crashed run leaves the file behind
        assert!(!send_to(&path, "show"), "nobody listens on a stale socket");
        let (tx, rx) = mpsc::channel();
        listen_at(&path, move |c| tx.send(c).unwrap()).unwrap();
        assert!(send_to(&path, "show"));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), "show");
    }
}
