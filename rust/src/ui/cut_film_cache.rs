//! §05-cut#1-screen "Pictures": the film strip's pictures cut ON THE FLY from the recording itself, at the second
//! each cell stands on, and kept in the temp folder (`<temp>/naivepost-thumbs/`) so a second visit costs nothing.
//! Prepare's frames are what the model reads; at the band's zoom they were either too sparse (one a second, or one
//! a scene) or missing on a recording Prepare had not reached. A cell whose picture is not cut yet shows Prepare's
//! frame for that second when there is one, else the slate, and the band repaints as pictures land.
//!
//! The seconds asked for sit on a grid that follows the zoom: [`FINEST_STEP_S`] doubled until one step covers a
//! cell, so a zoom in by two reuses every second picture already cut and a scroll asks only for the new edge.
//! Work goes newest-ask first (a fast scroll's stale asks are what is left waiting, and past [`QUEUE_MAX`] they are
//! dropped and asked again if their cell comes back), cut by [`WORKERS`] ffmpeg processes at a time.

use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};

/// The pictures are cut this tall: crisp up to the band's tallest row, about 8 kB each as JPEG.
pub const PICTURE_HEIGHT_PX: u32 = 180;
/// The finest grid the band asks on, the same 250 ms Prepare's frames sit on (§F1.6).
pub const FINEST_STEP_S: f64 = 0.25;
/// ffmpeg processes cutting at once.
const WORKERS: usize = 3;
/// Asks kept waiting; older ones are dropped (and asked again when their cell is drawn again).
const QUEUE_MAX: usize = 96;

/// One picture to cut: the recording, the second in it, and where the picture goes.
#[derive(Clone)]
pub(crate) struct Job {
    file: PathBuf,
    second: f64,
    out: PathBuf,
}

/// The shared queue the workers take from: newest at the end.
pub(crate) type Queue = Arc<(Mutex<Vec<Job>>, Condvar)>;

/// Where the pictures are kept: the temp folder's `naivepost-thumbs/`, one folder per recording file.
pub fn cache_root() -> PathBuf {
    std::env::temp_dir().join("naivepost-thumbs")
}

/// The grid step for a cell `cell_seconds` wide: [`FINEST_STEP_S`] doubled until one step covers the cell.
pub fn step_for(cell_seconds: f64) -> f64 {
    let mut step = FINEST_STEP_S;
    while step < cell_seconds && step < 3600.0 {
        step *= 2.0;
    }
    step
}

/// The second a cell asks for: its own, snapped to the step's grid.
pub fn snapped(second: f64, step: f64) -> f64 {
    ((second.max(0.0) / step).round() * step).max(0.0)
}

/// The picture of `file` at `second` (already snapped), if it is cut; otherwise asks for it once and answers
/// `None`, and the band repaints when it lands. A live draw's door: it never blocks.
pub fn picture_at(file: &Path, second: f64) -> Option<PathBuf> {
    let folder = folder_of(file)?;
    let out = folder.join(format!("{}.jpg", (second * 1000.0).round() as u64));
    if CUT_PAGE.with(|CutPage { present, .. }| present.borrow().contains(&out)) {
        return Some(out);
    }
    if CUT_PAGE.with(|CutPage { failed, .. }| failed.borrow().contains(&out))
        || CUT_PAGE.with(
            |CutPage {
                 film_cache_asked: asked,
                 ..
             }| asked.borrow().contains(&out),
        )
    {
        return None;
    }
    if out.is_file() {
        CUT_PAGE.with(|CutPage { present, .. }| present.borrow_mut().insert(out.clone()));
        return Some(out);
    }
    ask(Job {
        file: file.to_path_buf(),
        second,
        out,
    });
    None
}

/// How many pictures are cut and known, and how many are still asked for: what a test waits on.
pub fn counts_for_test() -> (usize, usize) {
    (
        CUT_PAGE.with(|CutPage { present, .. }| present.borrow().len()),
        CUT_PAGE.with(
            |CutPage {
                 film_cache_asked: asked,
                 ..
             }| asked.borrow().len(),
        ),
    )
}

/// The recording's cache folder: named by its path, size and mtime, so an edited or replaced file gets new pictures.
fn folder_of(file: &Path) -> Option<PathBuf> {
    if let Some(known) =
        CUT_PAGE.with(|CutPage { folders, .. }| folders.borrow().get(file).cloned())
    {
        return known;
    }
    let found = std::fs::metadata(file).ok().map(|meta| {
        use sha1::{Digest, Sha1};
        let mtime = meta
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |at| at.as_nanos());
        let mut hash = Sha1::new();
        hash.update(format!("{}|{}|{mtime}", file.display(), meta.len()).as_bytes());
        let digest = hash.finalize();
        let name: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
        cache_root().join(name)
    });
    CUT_PAGE.with(|CutPage { folders, .. }| {
        folders
            .borrow_mut()
            .insert(file.to_path_buf(), found.clone())
    });
    found
}

/// Queue one picture, starting the workers and the loop that collects their answers on the first ask.
fn ask(job: Job) {
    let queue = CUT_PAGE.with(|CutPage { queue: slot, .. }| {
        slot.borrow_mut()
            .get_or_insert_with(|| {
                let queue: Queue = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
                let (tell, told) = mpsc::channel::<(PathBuf, bool)>();
                for _ in 0..WORKERS {
                    let (queue, tell) = (queue.clone(), tell.clone());
                    std::thread::spawn(move || work(&queue, &tell));
                }
                collect(told);
                queue
            })
            .clone()
    });
    CUT_PAGE.with(
        |CutPage {
             film_cache_asked: asked,
             ..
         }| asked.borrow_mut().insert(job.out.clone()),
    );
    let (jobs, wake) = &*queue;
    let mut jobs = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    jobs.push(job);
    if jobs.len() > QUEUE_MAX {
        // The oldest asks are the cells a scroll has left: drop them, and let a redraw ask again.
        let surplus = jobs.len() - QUEUE_MAX;
        let dropped: Vec<Job> = jobs.drain(..surplus).collect();
        CUT_PAGE.with(
            |CutPage {
                 film_cache_asked: asked,
                 ..
             }| {
                let mut asked = asked.borrow_mut();
                for job in dropped {
                    asked.remove(&job.out);
                }
            },
        );
    }
    wake.notify_one();
}

/// One worker: take the newest ask, cut it, say how it went.
fn work(queue: &Queue, tell: &mpsc::Sender<(PathBuf, bool)>) {
    let (jobs, wake) = &**queue;
    loop {
        let job = {
            let mut jobs = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            loop {
                if let Some(job) = jobs.pop() {
                    break job;
                }
                jobs = wake
                    .wait(jobs)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let cut = cut_picture(&job);
        if tell.send((job.out, cut)).is_err() {
            return;
        }
    }
}

/// ffmpeg seeks to the second (before `-i`: a keyframe jump, then decoded up to the second) and writes one picture,
/// first under a temporary name and then renamed, so a draw never reads half a file.
fn cut_picture(job: &Job) -> bool {
    let Some(folder) = job.out.parent() else {
        return false;
    };
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    let partial = job.out.with_extension("part.jpg");
    let made = std::process::Command::new(crate::produce_upload_media::ffmpeg_program())
        .args(["-nostdin", "-v", "error", "-y", "-ss"])
        .arg(format!("{:.3}", job.second))
        .arg("-i")
        .arg(&job.file)
        .args(["-frames:v", "1", "-vf"])
        .arg(format!("scale=-2:{PICTURE_HEIGHT_PX}"))
        .args(["-q:v", "5"])
        .arg(&partial)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    // ffmpeg exits cleanly with no picture when the second is past the file's end.
    let landed = made
        && std::fs::metadata(&partial).is_ok_and(|meta| meta.len() > 0)
        && std::fs::rename(&partial, &job.out).is_ok();
    if !landed {
        let _ = std::fs::remove_file(&partial);
    }
    landed
}

/// On the GTK thread: take the workers' answers, note them, and repaint the band once per landed batch.
fn collect(told: mpsc::Receiver<(PathBuf, bool)>) {
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        let mut landed = false;
        while let Ok((out, cut)) = told.try_recv() {
            CUT_PAGE.with(
                |CutPage {
                     film_cache_asked: asked,
                     ..
                 }| asked.borrow_mut().remove(&out),
            );
            if cut {
                CUT_PAGE.with(|CutPage { present, .. }| present.borrow_mut().insert(out));
                landed = true;
            } else {
                CUT_PAGE.with(|CutPage { failed, .. }| failed.borrow_mut().insert(out));
            }
        }
        if landed {
            crate::ui::cut_picture_rows::repaint_bands();
        }
        glib::ControlFlow::Continue
    });
}
