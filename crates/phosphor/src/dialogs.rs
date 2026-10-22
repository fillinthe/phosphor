//! File dialogs that never pause the show. On macOS they open as sheets on the main window while
//! rendering continues; the chosen paths arrive a frame later through `poll`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

/// What a dialog was opened for, so the app knows what to do with the answer.
#[derive(Clone, Debug)]
pub enum Purpose {
    LayerSource(u64),
    ImportFiles,
    WatchFolder,
    OpenProject,
    SaveProject,
    SaveSnapshot,
    StreamFile,
    ExportShow,
    TouchOsc,
    /// A TouchOSC layout to bring in as Control Surface pages.
    ImportTouchOsc,
    ImportResolume,
    RelinkFolder,
    /// A 3D model (OBJ) for this mapping surface.
    Model3d(u64),
    /// A stage photo for this output (mapping editor background).
    StagePhoto(u64),
    /// A picture or movie for this mapping slice to show (`Composition::show_file_on_slice`).
    SliceFile(u64),
    /// A folder for this collection to watch.
    CollectionFolder(String),
    /// Save the cue list's names and timings as CSV / read them back.
    ExportCues,
    ImportCues,
    /// A text file for this text layer (shown a part at a time).
    TextFile(u64),
    /// A calendar (.ics) whose events start cues.
    CueCalendar,
    /// A project to merge into this one.
    MergeProject,
    /// A background picture for this output.
    OutputBackground(u64),
    /// A patch list (CSV) of LED fixtures.
    ImportFixtures,
}

pub struct Dialogs {
    tx: Sender<(Purpose, Vec<PathBuf>)>,
    rx: Receiver<(Purpose, Vec<PathBuf>)>,
    /// True while a dialog is showing (only one at a time).
    pub busy: bool,
}

impl Default for Dialogs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, busy: false }
    }
}

impl Dialogs {
    fn run<F>(&mut self, purpose: Purpose, future: F)
    where
        F: std::future::Future<Output = Vec<PathBuf>> + Send + 'static,
    {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let paths = pollster::block_on(future);
            let _ = tx.send((purpose, paths));
        });
    }

    fn dialog(filter: Option<(&str, &[&str])>) -> rfd::AsyncFileDialog {
        let d = rfd::AsyncFileDialog::new();
        match filter {
            Some((name, exts)) => d.add_filter(name, exts),
            None => d,
        }
    }

    pub fn pick_file(&mut self, purpose: Purpose, filter: Option<(&str, &[&str])>) {
        let f = Self::dialog(filter).pick_file();
        self.run(purpose, async move { f.await.map(|h| vec![h.path().to_path_buf()]).unwrap_or_default() });
    }

    pub fn pick_files(&mut self, purpose: Purpose, filter: Option<(&str, &[&str])>) {
        let f = Self::dialog(filter).pick_files();
        self.run(purpose, async move { f.await.map(|v| v.iter().map(|h| h.path().to_path_buf()).collect()).unwrap_or_default() });
    }

    pub fn pick_folder(&mut self, purpose: Purpose) {
        let f = rfd::AsyncFileDialog::new().pick_folder();
        self.run(purpose, async move { f.await.map(|h| vec![h.path().to_path_buf()]).unwrap_or_default() });
    }

    pub fn save_file(&mut self, purpose: Purpose, filter: (&str, &[&str]), name: &str) {
        let f = Self::dialog(Some(filter)).set_file_name(name).save_file();
        self.run(purpose, async move { f.await.map(|h| vec![h.path().to_path_buf()]).unwrap_or_default() });
    }

    /// Finished dialogs (cancelled ones come back with no paths).
    pub fn poll(&mut self) -> Vec<(Purpose, Vec<PathBuf>)> {
        let done: Vec<_> = self.rx.try_iter().collect();
        if !done.is_empty() {
            self.busy = false;
        }
        done.into_iter().filter(|(_, p)| !p.is_empty()).collect()
    }
}
