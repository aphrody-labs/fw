//! Worker-pool scheduler for file operations, after yazi-scheduler: every source becomes a task, a fixed
//! number of workers (yazi's `tasks.file_workers`) drain the queue, each task first collects its tree
//! (so totals are known), then copies, moves or removes it while emitting progress [`Event`]s.
//! Events serialise to one JSON object per line, so the stream is itself a file.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CHUNK: usize = 1 << 20;
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Copy,
    Move,
    Remove,
}

/// What to do when the destination already exists. yazi asks interactively; a pipeline cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// Count the file as failed and keep the destination.
    Fail,
    /// Keep the destination, count the file as done.
    Skip,
    Overwrite,
    /// Write to `name_1.ext`, `name_2.ext`, ... instead.
    Rename,
}

impl Conflict {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "fail" => Self::Fail,
            "skip" => Self::Skip,
            "overwrite" | "force" => Self::Overwrite,
            "rename" => Self::Rename,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub op: Op,
    pub src: PathBuf,
    /// Final destination path (see [`target_for`]); `None` for [`Op::Remove`].
    pub dst: Option<PathBuf>,
    pub conflict: Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Queued { id: u32, op: Op, src: String, dst: Option<String> },
    Collected { id: u32, files: u32, bytes: u64 },
    Progress { id: u32, files_done: u32, files_failed: u32, bytes_done: u64, bytes_total: u64 },
    Error { id: u32, path: String, message: String },
    Finished(Summary),
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub id: u32,
    pub op: Op,
    pub src: String,
    pub dst: Option<String>,
    pub status: Status,
    pub files_total: u32,
    pub files_done: u32,
    pub files_failed: u32,
    pub bytes_total: u64,
    pub bytes_done: u64,
    #[serde(skip)]
    pub errors: Vec<String>,
}

type Sink = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Scheduler {
    tx: Option<mpsc::Sender<(u32, Job)>>,
    handles: Vec<JoinHandle<()>>,
    sink: Sink,
    next: AtomicU32,
    cancels: Arc<Mutex<HashMap<u32, Arc<AtomicBool>>>>,
    results: Arc<Mutex<Vec<Summary>>>,
}

impl Scheduler {
    pub fn new(workers: usize, sink: impl Fn(Event) + Send + Sync + 'static) -> Self {
        let sink: Sink = Arc::new(sink);
        let (tx, rx) = mpsc::channel::<(u32, Job)>();
        let rx = Arc::new(Mutex::new(rx));
        let cancels: Arc<Mutex<HashMap<u32, Arc<AtomicBool>>>> = Arc::default();
        let results: Arc<Mutex<Vec<Summary>>> = Arc::default();
        let handles = (0..workers.max(1))
            .map(|_| {
                let (rx, sink, cancels, results) = (rx.clone(), sink.clone(), cancels.clone(), results.clone());
                thread::spawn(move || {
                    loop {
                        let msg = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok((id, job)) = msg else { break };
                        let flag = cancels.lock().unwrap_or_else(|e| e.into_inner()).get(&id).cloned().unwrap_or_default();
                        let summary = run(id, &job, &flag, &sink);
                        sink(Event::Finished(summary.clone()));
                        results.lock().unwrap_or_else(|e| e.into_inner()).push(summary);
                    }
                })
            })
            .collect();
        Self { tx: Some(tx), handles, sink, next: AtomicU32::new(1), cancels, results }
    }

    pub fn submit(&self, job: Job) -> u32 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.cancels.lock().unwrap_or_else(|e| e.into_inner()).insert(id, Arc::default());
        (self.sink)(Event::Queued { id, op: job.op, src: lossy(&job.src), dst: job.dst.as_deref().map(lossy) });
        if let Some(tx) = &self.tx {
            tx.send((id, job)).ok();
        }
        id
    }

    /// Stop a task at its next chunk boundary; partial destination files are removed.
    pub fn cancel(&self, id: u32) {
        if let Some(flag) = self.cancels.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel_all(&self) {
        for flag in self.cancels.lock().unwrap_or_else(|e| e.into_inner()).values() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Close the queue, wait for every task and return the summaries ordered by id.
    pub fn finish(mut self) -> Vec<Summary> {
        self.tx.take();
        for h in self.handles.drain(..) {
            h.join().ok();
        }
        let mut out = std::mem::take(&mut *self.results.lock().unwrap_or_else(|e| e.into_inner()));
        out.sort_by_key(|s| s.id);
        out
    }
}

/// `cp`-style destination: into `dst` when it is a directory (or when several sources were given), else `dst` itself.
pub fn target_for(src: &Path, dst: &Path, many_sources: bool) -> io::Result<PathBuf> {
    if dst.is_dir() || many_sources {
        if many_sources && !dst.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("{}: not a directory (several sources given)", dst.display())));
        }
        let name = src.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: has no file name", src.display())))?;
        Ok(dst.join(name))
    } else {
        Ok(dst.to_path_buf())
    }
}

fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

// ---- one task ----

struct Task<'a> {
    id: u32,
    cancel: &'a AtomicBool,
    sink: &'a Sink,
    sum: Summary,
    last: Instant,
}

impl Task<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn progress(&mut self, force: bool) {
        if force || self.last.elapsed() >= PROGRESS_EVERY {
            self.last = Instant::now();
            (self.sink)(Event::Progress { id: self.id, files_done: self.sum.files_done, files_failed: self.sum.files_failed, bytes_done: self.sum.bytes_done, bytes_total: self.sum.bytes_total });
        }
    }

    fn fail(&mut self, path: &Path, err: impl std::fmt::Display) {
        let message = err.to_string();
        self.sum.files_failed += 1;
        self.sum.errors.push(format!("{}: {message}", path.display()));
        (self.sink)(Event::Error { id: self.id, path: lossy(path), message });
    }
}

enum Item {
    Dir { src: PathBuf, dst: PathBuf },
    File { src: PathBuf, dst: PathBuf, size: u64 },
    Link { src: PathBuf, dst: PathBuf },
}

impl Item {
    fn size(&self) -> u64 {
        if let Self::File { size, .. } = self { *size } else { 0 }
    }
}

/// Pre-order walk (parents before children); symlinks are items, never followed.
fn collect(src: &Path, dst: &Path, out: &mut Vec<Item>) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        out.push(Item::Link { src: src.into(), dst: dst.into() });
    } else if meta.is_dir() {
        out.push(Item::Dir { src: src.into(), dst: dst.into() });
        let mut names: Vec<_> = fs::read_dir(src)?.collect::<Result<Vec<_>, _>>()?.into_iter().map(|e| e.file_name()).collect();
        names.sort();
        for n in names {
            collect(&src.join(&n), &dst.join(&n), out)?;
        }
    } else {
        out.push(Item::File { src: src.into(), dst: dst.into(), size: meta.len() });
    }
    Ok(())
}

fn run(id: u32, job: &Job, cancel: &AtomicBool, sink: &Sink) -> Summary {
    let sum = Summary { id, op: job.op, src: lossy(&job.src), dst: job.dst.as_deref().map(lossy), status: Status::Done, files_total: 0, files_done: 0, files_failed: 0, bytes_total: 0, bytes_done: 0, errors: Vec::new() };
    let mut t = Task { id, cancel, sink, sum, last: Instant::now() };
    let result = match job.op {
        Op::Remove => remove_task(&mut t, &job.src),
        Op::Copy | Op::Move => match &job.dst {
            Some(dst) => transfer_task(&mut t, job, dst),
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, "copy and move need a destination")),
        },
    };
    if let Err(e) = result {
        t.fail(&job.src, e);
    }
    t.sum.status = if t.cancelled() { Status::Cancelled } else if t.sum.files_failed > 0 { Status::Failed } else { Status::Done };
    t.progress(true);
    t.sum
}

fn begin(t: &mut Task, items: &[Item]) {
    t.sum.files_total = items.len() as u32;
    t.sum.bytes_total = items.iter().map(Item::size).sum();
    (t.sink)(Event::Collected { id: t.id, files: t.sum.files_total, bytes: t.sum.bytes_total });
}

fn remove_task(t: &mut Task, src: &Path) -> io::Result<()> {
    let mut items = Vec::new();
    collect(src, src, &mut items)?;
    begin(t, &items);
    remove_items(t, &items, true);
    Ok(())
}

/// Delete children before parents.
fn remove_items(t: &mut Task, items: &[Item], count: bool) {
    for item in items.iter().rev() {
        if t.cancelled() {
            return;
        }
        let (path, is_dir) = match item {
            Item::Dir { src, .. } => (src, true),
            Item::File { src, .. } | Item::Link { src, .. } => (src, false),
        };
        match remove_path(path, is_dir) {
            Ok(()) if count => {
                t.sum.files_done += 1;
                t.sum.bytes_done += item.size();
                t.progress(false);
            }
            Ok(()) => {}
            Err(e) => t.fail(path, e),
        }
    }
}

fn remove_path(path: &Path, is_dir: bool) -> io::Result<()> {
    let attempt = |p: &Path| if is_dir { fs::remove_dir(p) } else { fs::remove_file(p).or_else(|e| if cfg!(windows) { fs::remove_dir(p).map_err(|_| e) } else { Err(e) }) };
    match attempt(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // Read-only files (Windows, git objects) refuse deletion until the flag is cleared.
            let mut perms = fs::symlink_metadata(path)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(path, perms)?;
            attempt(path)
        }
        other => other,
    }
}

fn transfer_task(t: &mut Task, job: &Job, dst: &Path) -> io::Result<()> {
    guard_self_copy(&job.src, dst)?;
    let dst = if job.conflict == Conflict::Rename && fs::symlink_metadata(dst).is_ok() { unique_name(dst) } else { dst.to_path_buf() };

    if job.op == Op::Move {
        let src_is_dir = fs::symlink_metadata(&job.src)?.is_dir();
        let dst_meta = fs::symlink_metadata(&dst);
        let merge = src_is_dir && dst_meta.as_ref().is_ok_and(|m| m.is_dir());
        if !merge {
            match (dst_meta.is_ok(), job.conflict) {
                (true, Conflict::Fail) => return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} exists", dst.display()))),
                (true, Conflict::Skip) => {
                    begin_single(t, &job.src);
                    t.sum.files_done = 1;
                    return Ok(());
                }
                _ => {}
            }
            // Same filesystem: a rename moves the whole tree at once.
            if fs::rename(&job.src, &dst).is_ok() {
                begin_single(t, &job.src);
                t.sum.files_done = 1;
                t.sum.bytes_done = t.sum.bytes_total;
                return Ok(());
            }
        }
    }

    let mut items = Vec::new();
    collect(&job.src, &dst, &mut items)?;
    begin(t, &items);
    for item in &items {
        if t.cancelled() {
            return Ok(());
        }
        match copy_item(t, item, job.conflict) {
            Ok(()) => t.sum.files_done += 1,
            Err(e) => t.fail(item_src(item), e),
        }
        t.progress(false);
    }
    if job.op == Op::Move && t.sum.files_failed == 0 && !t.cancelled() {
        remove_items(t, &items, false);
    }
    Ok(())
}

fn begin_single(t: &mut Task, src: &Path) {
    t.sum.files_total = 1;
    t.sum.bytes_total = fs::symlink_metadata(src).map(|m| if m.is_dir() { 0 } else { m.len() }).unwrap_or(0);
    (t.sink)(Event::Collected { id: t.id, files: 1, bytes: t.sum.bytes_total });
}

fn item_src(item: &Item) -> &Path {
    match item {
        Item::Dir { src, .. } | Item::File { src, .. } | Item::Link { src, .. } => src,
    }
}

fn guard_self_copy(src: &Path, dst: &Path) -> io::Result<()> {
    let s = fs::canonicalize(src)?;
    let parent = dst.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let d = fs::canonicalize(parent)?.join(dst.file_name().unwrap_or_default());
    if d == s || (s.is_dir() && d.starts_with(&s)) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("cannot place {} inside itself", src.display())));
    }
    Ok(())
}

fn unique_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|n| path.with_file_name(format!("{stem}_{n}{ext}")))
        .find(|p| fs::symlink_metadata(p).is_err())
        .unwrap_or_else(|| path.to_path_buf())
}

fn copy_item(t: &mut Task, item: &Item, conflict: Conflict) -> io::Result<()> {
    match item {
        Item::Dir { dst, .. } => match fs::symlink_metadata(dst) {
            Ok(m) if m.is_dir() => Ok(()),
            Ok(_) if conflict == Conflict::Overwrite => {
                fs::remove_file(dst)?;
                fs::create_dir(dst)
            }
            Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} exists and is not a directory", dst.display()))),
            Err(_) => fs::create_dir_all(dst),
        },
        Item::File { src, dst, size } => {
            if let Ok(m) = fs::symlink_metadata(dst) {
                match conflict {
                    Conflict::Skip => {
                        t.sum.bytes_done += size;
                        return Ok(());
                    }
                    Conflict::Overwrite if !m.is_dir() => fs::remove_file(dst)?,
                    _ => return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} exists", dst.display()))),
                }
            }
            copy_file(t, src, dst)
        }
        Item::Link { src, dst } => {
            if fs::symlink_metadata(dst).is_ok() {
                match conflict {
                    Conflict::Skip => return Ok(()),
                    Conflict::Overwrite => remove_path(dst, false)?,
                    _ => return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} exists", dst.display()))),
                }
            }
            let target = fs::read_link(src)?;
            symlink(&target, dst, src)
        }
    }
}

#[cfg(unix)]
fn symlink(target: &Path, dst: &Path, _src: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, dst)
}

#[cfg(windows)]
fn symlink(target: &Path, dst: &Path, src: &Path) -> io::Result<()> {
    if fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false) { std::os::windows::fs::symlink_dir(target, dst) } else { std::os::windows::fs::symlink_file(target, dst) }
}

fn copy_file(t: &mut Task, src: &Path, dst: &Path) -> io::Result<()> {
    let mut input = fs::File::open(src)?;
    let meta = input.metadata()?;
    let mut output = fs::File::create(dst)?;
    let mut buf = vec![0u8; CHUNK];
    let result = (|| -> io::Result<()> {
        loop {
            if t.cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            output.write_all(&buf[..n])?;
            t.sum.bytes_done += n as u64;
            t.progress(false);
        }
    })();
    drop(output);
    match result {
        Ok(()) => {
            fs::set_permissions(dst, meta.permissions()).ok();
            if let Ok(modified) = meta.modified() {
                fs::File::options().write(true).open(dst).and_then(|f| f.set_modified(modified)).ok();
            }
            Ok(())
        }
        Err(e) => {
            fs::remove_file(dst).ok();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Log = Arc<Mutex<Vec<Event>>>;

    fn sched(workers: usize) -> (Scheduler, Log) {
        let log: Log = Arc::default();
        let l = log.clone();
        (Scheduler::new(workers, move |e| l.lock().unwrap().push(e)), log)
    }

    fn tree(root: &Path) {
        fs::create_dir_all(root.join("src/deep")).unwrap();
        fs::write(root.join("src/a.txt"), b"alpha").unwrap();
        fs::write(root.join("src/deep/b.bin"), vec![7u8; 3 * CHUNK + 5]).unwrap();
    }

    fn job(op: Op, src: &Path, dst: Option<&Path>, conflict: Conflict) -> Job {
        Job { op, src: src.into(), dst: dst.map(Into::into), conflict }
    }

    #[test]
    fn copy_tree_reports_totals_and_progress() {
        let d = tempfile::tempdir().unwrap();
        tree(d.path());
        let (s, log) = sched(3);
        s.submit(job(Op::Copy, &d.path().join("src"), Some(&d.path().join("out")), Conflict::Fail));
        let sums = s.finish();
        assert_eq!(sums[0].status, Status::Done);
        assert_eq!((sums[0].files_total, sums[0].files_done, sums[0].bytes_total, sums[0].bytes_done), (4, 4, 5 + 3 * CHUNK as u64 + 5, 5 + 3 * CHUNK as u64 + 5));
        assert_eq!(fs::read(d.path().join("out/a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(d.path().join("out/deep/b.bin")).unwrap().len(), 3 * CHUNK + 5);
        assert!(d.path().join("src/a.txt").exists());

        let log = log.lock().unwrap();
        assert!(matches!(log.first(), Some(Event::Queued { id: 1, op: Op::Copy, .. })));
        assert!(log.iter().any(|e| matches!(e, Event::Collected { files: 4, .. })));
        assert!(matches!(log.last(), Some(Event::Finished(s)) if s.status == Status::Done));
        // Every event is a single JSON line with an "event" tag.
        for e in log.iter() {
            let line = serde_json::to_string(e).unwrap();
            assert!(!line.contains('\n') && line.starts_with("{\"event\":\""), "{line}");
        }
    }

    #[test]
    fn move_renames_within_a_filesystem() {
        let d = tempfile::tempdir().unwrap();
        tree(d.path());
        let (s, _) = sched(1);
        s.submit(job(Op::Move, &d.path().join("src"), Some(&d.path().join("moved")), Conflict::Fail));
        let sums = s.finish();
        assert_eq!((sums[0].status, sums[0].files_done), (Status::Done, 1));
        assert!(!d.path().join("src").exists());
        assert_eq!(fs::read(d.path().join("moved/a.txt")).unwrap(), b"alpha");
    }

    #[test]
    fn move_onto_existing_directory_merges_and_removes_the_source() {
        let d = tempfile::tempdir().unwrap();
        tree(d.path());
        fs::create_dir_all(d.path().join("dst")).unwrap();
        fs::write(d.path().join("dst/keep.txt"), b"keep").unwrap();
        let (s, _) = sched(1);
        s.submit(job(Op::Move, &d.path().join("src"), Some(&d.path().join("dst")), Conflict::Fail));
        let sums = s.finish();
        assert_eq!(sums[0].status, Status::Done, "{:?}", sums[0].errors);
        assert!(!d.path().join("src").exists());
        assert_eq!(fs::read(d.path().join("dst/keep.txt")).unwrap(), b"keep");
        assert_eq!(fs::read(d.path().join("dst/a.txt")).unwrap(), b"alpha");
    }

    #[test]
    fn remove_deletes_children_before_parents() {
        let d = tempfile::tempdir().unwrap();
        tree(d.path());
        let (s, _) = sched(2);
        s.submit(job(Op::Remove, &d.path().join("src"), None, Conflict::Fail));
        let sums = s.finish();
        assert_eq!((sums[0].status, sums[0].files_done, sums[0].files_total), (Status::Done, 4, 4));
        assert!(!d.path().join("src").exists());
    }

    #[test]
    fn conflict_policies() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("a.txt"), b"new").unwrap();
        fs::write(d.path().join("b.txt"), b"old").unwrap();
        let (a, b) = (d.path().join("a.txt"), d.path().join("b.txt"));

        let run_one = |c: Conflict| {
            let (s, _) = sched(1);
            s.submit(job(Op::Copy, &a, Some(&b), c));
            s.finish().remove(0)
        };
        let failed = run_one(Conflict::Fail);
        assert_eq!((failed.status, failed.files_failed), (Status::Failed, 1));
        assert!(failed.errors[0].contains("exists"));
        assert_eq!(fs::read(&b).unwrap(), b"old");

        assert_eq!(run_one(Conflict::Skip).status, Status::Done);
        assert_eq!(fs::read(&b).unwrap(), b"old");

        assert_eq!(run_one(Conflict::Rename).status, Status::Done);
        assert_eq!(fs::read(d.path().join("b_1.txt")).unwrap(), b"new");
        assert_eq!(fs::read(&b).unwrap(), b"old");

        assert_eq!(run_one(Conflict::Overwrite).status, Status::Done);
        assert_eq!(fs::read(&b).unwrap(), b"new");
    }

    #[test]
    fn missing_source_and_self_copy_fail_cleanly() {
        let d = tempfile::tempdir().unwrap();
        tree(d.path());
        let (s, _) = sched(2);
        s.submit(job(Op::Copy, &d.path().join("nope"), Some(&d.path().join("x")), Conflict::Fail));
        s.submit(job(Op::Copy, &d.path().join("src"), Some(&d.path().join("src/deep/inner")), Conflict::Fail));
        let sums = s.finish();
        assert!(sums.iter().all(|s| s.status == Status::Failed));
        assert!(sums[1].errors[0].contains("inside itself"), "{:?}", sums[1].errors);
        assert!(!d.path().join("src/deep/inner").exists());
    }

    #[test]
    fn cancelling_stops_the_task_and_removes_the_partial_file() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("big"), vec![1u8; 4 * CHUNK]).unwrap();
        // The worker parks on the Collected event until the test has cancelled, so the order is deterministic.
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let gate = Mutex::new(gate_rx);
        let s = Scheduler::new(1, move |e| {
            if matches!(e, Event::Collected { .. }) {
                gate.lock().unwrap().recv().ok();
            }
        });
        let id = s.submit(job(Op::Copy, &d.path().join("big"), Some(&d.path().join("copy")), Conflict::Fail));
        s.cancel(id);
        gate_tx.send(()).unwrap();
        let sums = s.finish();
        assert_eq!(sums[0].status, Status::Cancelled);
        assert!(!d.path().join("copy").exists());
        assert!(d.path().join("big").exists());
    }

    #[test]
    fn many_tasks_run_on_the_pool_and_target_resolution_follows_cp() {
        let d = tempfile::tempdir().unwrap();
        for n in 0..8 {
            fs::write(d.path().join(format!("f{n}")), format!("{n}")).unwrap();
        }
        fs::create_dir(d.path().join("out")).unwrap();
        let (s, _) = sched(3);
        for n in 0..8 {
            let src = d.path().join(format!("f{n}"));
            let dst = target_for(&src, &d.path().join("out"), true).unwrap();
            s.submit(job(Op::Copy, &src, Some(&dst), Conflict::Fail));
        }
        let sums = s.finish();
        assert_eq!(sums.len(), 8);
        assert!(sums.iter().all(|s| s.status == Status::Done));
        assert_eq!(sums.iter().map(|s| s.id).collect::<Vec<_>>(), (1..=8).collect::<Vec<_>>());
        assert_eq!(fs::read_dir(d.path().join("out")).unwrap().count(), 8);

        let single = target_for(&d.path().join("f0"), &d.path().join("renamed"), false).unwrap();
        assert_eq!(single, d.path().join("renamed"));
        assert!(target_for(&d.path().join("f0"), &d.path().join("missing"), true).is_err());
    }
}
