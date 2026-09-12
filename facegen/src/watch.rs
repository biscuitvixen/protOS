//! File watching for hot reload.
//!
//! One thread owns an inotify watcher over the assets directory and
//! turns bursts of filesystem events (editors write, rename and touch
//! in quick succession) into single reload requests on the control
//! channel, one per kind of file changed.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use notify::{RecursiveMode, Watcher};

use crate::app::Control;

/// Quiet period after the last event before a reload fires.
const DEBOUNCE: Duration = Duration::from_millis(120);

/// Which assets a changed path belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Shaders,
    Faces,
}

pub fn classify(assets: &Path, path: &Path) -> Option<Kind> {
    let rel = path.strip_prefix(assets).ok()?;
    match rel.components().next()?.as_os_str().to_str()? {
        "shaders" if path.extension().is_some_and(|e| e == "wgsl") => Some(Kind::Shaders),
        "faces" if path.extension().is_some_and(|e| e == "toml") => Some(Kind::Faces),
        _ => None,
    }
}

/// Watch `assets/shaders` and `assets/faces`, sending a reload control
/// for each burst of changes. The thread ends when the control channel
/// closes.
pub fn start(
    assets: PathBuf,
    control: mpsc::Sender<Control>,
) -> anyhow::Result<thread::JoinHandle<()>> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // Reads by the reload itself show up as access events; only
        // content changes count.
        if let Ok(event) = event
            && matches!(
                event.kind,
                notify::EventKind::Modify(_)
                    | notify::EventKind::Create(_)
                    | notify::EventKind::Remove(_)
            )
        {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    })
    .context("creating the file watcher")?;
    for sub in ["shaders", "faces"] {
        let dir = assets.join(sub);
        if dir.is_dir() {
            watcher
                .watch(&dir, RecursiveMode::Recursive)
                .with_context(|| format!("watching {}", dir.display()))?;
        }
    }
    tracing::info!(assets = %assets.display(), "watching for changes");
    Ok(thread::Builder::new().name("watch".into()).spawn(move || {
        let _keep_alive = watcher;
        let mut pending = [false; 2];
        let mut deadline: Option<Instant> = None;
        loop {
            let wait = deadline.map_or(Duration::from_secs(3600), |d| {
                d.saturating_duration_since(Instant::now())
            });
            match rx.recv_timeout(wait) {
                Ok(path) => {
                    if let Some(kind) = classify(&assets, &path) {
                        pending[kind as usize] = true;
                        deadline = Some(Instant::now() + DEBOUNCE);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    deadline = None;
                    for (i, flag) in pending.iter_mut().enumerate() {
                        if std::mem::take(flag) {
                            let message = if i == Kind::Shaders as usize {
                                Control::ReloadShaders
                            } else {
                                Control::ReloadFace
                            };
                            if control.send(message).is_err() {
                                return;
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_shader_and_face_files_under_the_assets_directory_are_classified() {
        let assets = Path::new("/a");
        assert_eq!(
            classify(assets, Path::new("/a/shaders/features/eye.wgsl")),
            Some(Kind::Shaders),
            "wgsl under shaders"
        );
        assert_eq!(
            classify(assets, Path::new("/a/faces/default.toml")),
            Some(Kind::Faces),
            "toml under faces"
        );
        assert_eq!(
            classify(assets, Path::new("/a/shaders/notes.md")),
            None,
            "other files ignored"
        );
        assert_eq!(
            classify(assets, Path::new("/b/shaders/face.wgsl")),
            None,
            "outside the assets root ignored"
        );
    }

    #[test]
    fn a_burst_of_writes_becomes_one_reload_request() {
        let dir = std::env::temp_dir().join(format!("facegen-watch-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("shaders")).unwrap();
        let (tx, rx) = mpsc::channel();
        let _thread = start(dir.clone(), tx).unwrap();
        thread::sleep(Duration::from_millis(100));
        for i in 0..5 {
            std::fs::write(dir.join("shaders/face.wgsl"), format!("// {i}")).unwrap();
        }
        let first = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("a reload should arrive");
        assert!(
            matches!(first, Control::ReloadShaders),
            "shader change should request a shader reload"
        );
        assert!(
            rx.recv_timeout(Duration::from_millis(400)).is_err(),
            "the burst should have been coalesced into one request"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
