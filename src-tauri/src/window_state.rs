// Copyright 2026 The Kstack Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `<state>/window-state.json`: the main and log windows' last frames. A missing,
//! corrupt or unknown-version file reads as no saved frames.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::Folders;

/// Stamped into the file on every write; a file with another stamp is ignored.
const SCHEMA_VERSION: u32 = 2;

/// Also deleted by `make clean` (`scripts/clean-dev-data.go`).
const FILE_NAME: &str = "window-state.json";

/// Where window frames are kept across launches: in the state
/// directory, so a dev build keeps its own and `make clean-data` resets it.
pub fn path(folders: &Folders) -> PathBuf {
    folders.state.join(FILE_NAME)
}

/// A window's outer position and inner size in logical px, each monitor's
/// origin being its reported position over its own scale: the space the
/// window builder places windows in.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The main window's frame and whether it was maximized. `frame` is the
/// unmaximized frame, so unmaximizing a restored window returns to it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct SavedFrame {
    #[serde(flatten)]
    pub frame: Frame,
    pub maximized: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowStateFile {
    schema_version: u32,
    main: Option<SavedFrame>,
    #[serde(default)]
    logs: Option<Frame>,
}

fn read_file(path: &Path) -> Option<WindowStateFile> {
    let contents = std::fs::read_to_string(path).ok()?;
    let file: WindowStateFile = serde_json::from_str(&contents).ok()?;
    (file.schema_version == 1 || file.schema_version == SCHEMA_VERSION).then_some(file)
}

/// The saved main frame, including from a version 1 file.
pub fn read(path: &Path) -> Option<SavedFrame> {
    read_file(path)?.main
}

/// The saved log-window frame, when this file has one.
pub fn read_logs(path: &Path) -> Option<Frame> {
    let file = read_file(path)?;
    (file.schema_version == SCHEMA_VERSION)
        .then_some(file.logs)
        .flatten()
}

/// Writes the main frame, retaining the log frame in the same file.
pub fn write(path: &Path, saved: &SavedFrame) -> std::io::Result<()> {
    write_file(path, Some(*saved), read_logs(path))
}

/// Writes the log frame, retaining the main frame in the same file.
pub fn write_logs(path: &Path, frame: &Frame) -> std::io::Result<()> {
    write_file(path, read(path), Some(*frame))
}

/// Replaces both frames atomically (temp + rename), creating the parent dir.
fn write_file(path: &Path, main: Option<SavedFrame>, logs: Option<Frame>) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("window state path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;

    let file = WindowStateFile {
        schema_version: SCHEMA_VERSION,
        main,
        logs,
    };
    let contents = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;

    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// What to remember after the main window moved or resized to `current`.
/// While maximized, the frame stays the one it will unmaximize to.
pub fn next_saved(previous: Option<SavedFrame>, current: SavedFrame) -> SavedFrame {
    match previous {
        Some(previous) if current.maximized => SavedFrame {
            frame: previous.frame,
            ..current
        },
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{folders, Scratch};

    const FRAME: Frame = Frame {
        x: 100.0,
        y: 50.0,
        width: 1200.0,
        height: 760.0,
    };

    const SCREEN: Frame = Frame {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn saved(frame: Frame, maximized: bool) -> SavedFrame {
        SavedFrame { frame, maximized }
    }

    #[test]
    fn the_frame_is_kept_in_the_state_directory() {
        let scratch = Scratch::new();
        let folders = folders(scratch.path());

        assert_eq!(path(&folders), folders.state.join("window-state.json"));
    }

    #[test]
    fn a_saved_frame_round_trips() {
        let scratch = Scratch::new();
        // The parent does not exist yet: the first write makes it.
        let path = scratch.path().join("config").join("window-state.json");

        write(&path, &saved(FRAME, true)).unwrap();

        assert_eq!(read(&path), Some(saved(FRAME, true)));
    }

    /// The format on disk. A change here is a change to a persisted format:
    /// bump `SCHEMA_VERSION` and update `docs/persisted-data.md`.
    #[test]
    fn the_file_is_stamped_with_its_schema_version() {
        let scratch = Scratch::new();
        let path = scratch.path().join("window-state.json");
        write(&path, &saved(FRAME, false)).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "schemaVersion": 2,
                "main": {
                    "x": 100.0, "y": 50.0, "width": 1200.0, "height": 760.0,
                    "maximized": false,
                },
                "logs": null,
            })
        );
    }

    #[test]
    fn a_missing_corrupt_or_differently_stamped_file_reads_as_nothing() {
        let scratch = Scratch::new();
        let path = scratch.path().join("window-state.json");
        assert_eq!(read(&path), None, "missing");

        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(read(&path), None, "corrupt");

        write(&path, &saved(FRAME, false)).unwrap();
        let other_version = std::fs::read_to_string(&path)
            .unwrap()
            .replace(r#""schemaVersion": 2"#, r#""schemaVersion": 3"#);
        std::fs::write(&path, other_version).unwrap();
        assert_eq!(read(&path), None, "another version");
    }

    #[test]
    fn an_unmaximized_window_remembers_its_frame() {
        assert_eq!(next_saved(None, saved(FRAME, false)), saved(FRAME, false));
    }

    #[test]
    fn a_maximized_window_keeps_the_frame_it_unmaximizes_to() {
        let next = next_saved(Some(saved(FRAME, false)), saved(SCREEN, true));

        assert_eq!(next, saved(FRAME, true));
    }

    #[test]
    fn a_window_maximized_with_nothing_known_remembers_its_frame() {
        assert_eq!(next_saved(None, saved(FRAME, true)), saved(FRAME, true));
    }

    #[test]
    fn both_frames_survive_writes_in_either_order() {
        let scratch = Scratch::new();
        let path = scratch.path().join("window-state.json");
        write_logs(&path, &FRAME).unwrap();
        assert_eq!(read_logs(&path), Some(FRAME));
        assert_eq!(read(&path), None);

        write(&path, &saved(SCREEN, true)).unwrap();
        assert_eq!(read_logs(&path), Some(FRAME));
        assert_eq!(read(&path), Some(saved(SCREEN, true)));

        write_logs(&path, &SCREEN).unwrap();
        assert_eq!(read(&path), Some(saved(SCREEN, true)));
        assert_eq!(read_logs(&path), Some(SCREEN));
    }

    #[test]
    fn a_version_one_main_frame_survives_the_log_frame_upgrade() {
        let scratch = Scratch::new();
        let path = scratch.path().join("window-state.json");
        std::fs::write(&path, r#"{"schemaVersion":1,"main":{"x":100,"y":50,"width":1200,"height":760,"maximized":false}}"#).unwrap();
        assert_eq!(read(&path), Some(saved(FRAME, false)));
        assert_eq!(read_logs(&path), None);
        write_logs(&path, &SCREEN).unwrap();
        assert_eq!(read(&path), Some(saved(FRAME, false)));
        assert_eq!(read_logs(&path), Some(SCREEN));

        std::fs::write(
            &path,
            r#"{"schemaVersion":1,"main":null,"logs":{"x":0,"y":0,"width":900,"height":600}}"#,
        )
        .unwrap();
        assert_eq!(read_logs(&path), None, "version 1 had no log frame");
    }
}
