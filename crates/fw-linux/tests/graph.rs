//! The code graph on a hermetic mini kernel tree (`tests/fixtures/tree`): exact rows for every idiom and every query kind.

use std::path::{Path, PathBuf};

use fw_linux::graph;
use serde_json::{Value, json};

fn fixture_tree() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tree")
}

fn run(database: &Path, arguments: &[&str]) -> Value {
    let arguments: Vec<String> = arguments.iter().map(|argument| (*argument).to_owned()).collect();
    graph::cli(&arguments, &fixture_tree(), database).expect("graph command")
}

fn built() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("tempdir");
    let database = directory.path().join("graph.db");
    run(&database, &["build"]);
    (directory, database)
}

/// `name kind file:line` of every result row.
fn brief(answer: &Value) -> Vec<String> {
    answer["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| format!("{} {} {}", row["name"].as_str().unwrap_or(""), row["kind"].as_str().unwrap_or(""), row["location"].as_str().unwrap_or("")))
        .collect()
}

fn first(answer: &Value) -> &Value {
    &answer["results"][0]
}

#[test]
fn build_reports_counts_and_stores_the_tree_label() {
    let directory = tempfile::tempdir().expect("tempdir");
    let database = directory.path().join("graph.db");
    let report = run(&database, &["build", "--commit", "0123456789abcdef"]);
    assert_eq!(report["ok"], true);
    assert_eq!(report["tree"], json!({ "label": "v7.3.5", "version": "7.3.5", "commit": "0123456789abcdef" }));
    let counts = &report["counts"];
    assert_eq!(counts["files"], 17);
    assert_eq!(counts["symbols_by_kind"]["function"], 16);
    assert_eq!(counts["symbols_by_kind"]["kconfig"], 5);
    assert_eq!(counts["symbols_by_kind"]["menu"], 1);
    assert_eq!(counts["symbols_by_kind"]["enumerator"], 3);
    assert_eq!(counts["exports"], 3);
    assert_eq!(counts["registrations"], 2);
    assert_eq!(counts["build_edges"], 6);
    assert_eq!(counts["includes"], 14);
    assert_eq!(counts["includes_resolved"], 12);
    assert!(!database.with_file_name("graph.db.building").exists());

    let stats = run(&database, &["stats"]);
    assert_eq!(stats["meta"]["schema_version"], "1");
    assert_eq!(stats["meta"]["label"], "v7.3.5");
    assert_eq!(stats["meta"]["commit"], "0123456789abcdef");
    assert_eq!(stats["tables"]["files"], 17);
    assert!(stats["meta"]["built_at"].as_str().is_some_and(|stamp| stamp.ends_with('Z')));
}

#[test]
fn syscalls_define_sys_names() {
    let (_directory, database) = built();
    let answer = run(&database, &["def", "sys_read"]);
    assert_eq!(brief(&answer), ["sys_read function fs/read_write.c:32"]);
    assert_eq!(first(&answer)["signature"], "SYSCALL_DEFINE3(read, unsigned int, fd, char __user *, buf, size_t, count)");
    assert_eq!(first(&answer)["end_line"], 35);
    assert_eq!(brief(&run(&database, &["def", "compat_sys_read"])), ["compat_sys_read function fs/read_write.c:37"]);
    assert_eq!(brief(&run(&database, &["def", "sys_getpid"])), ["sys_getpid function fs/read_write.c:42"]);
    assert_eq!(run(&database, &["callees", "sys_read"])["results"][0]["callee"], "ksys_read");
}

#[test]
fn statement_macros_and_type_arguments_parse_cleanly() {
    let (_directory, database) = built();
    let answer = run(&database, &["file", "fs/idioms.c"]);
    assert_eq!(answer["file"]["parse_errors"], 0);
    assert_eq!(
        brief(&answer),
        [
            "idiom_entry struct fs/idioms.c:4",
            "idiom_fops variable fs/idioms.c:9",
            "idiom_read function fs/idioms.c:16",
            "idiom_after function fs/idioms.c:29",
            "sys_idiom function fs/idioms.c:34",
        ]
    );
    assert_eq!(first(&run(&database, &["def", "sys_idiom"]))["signature"], "SYSCALL_DEFINE2(idiom, int, fd, const char __user *, name)");
    let callees = run(&database, &["callees", "idiom_read"]);
    let names: Vec<&str> = callees["results"].as_array().expect("results").iter().map(|row| row["callee"].as_str().unwrap_or("")).collect();
    assert_eq!(names, ["list_for_each_entry", "container_of", "for_each_online_cpu", "per_cpu"]);
    assert_eq!(run(&database, &["refs", "idiom_read"])["results"][0]["in"], "idiom_fops");
    assert_eq!(run(&database, &["refs", "idiom_boot"])["registrations"][0]["location"], "fs/idioms.c:41");
}

#[test]
fn exports_annotations_and_storage() {
    let (_directory, database) = built();
    let vfs_read = run(&database, &["def", "vfs_read"]);
    assert_eq!(brief(&vfs_read), ["vfs_read function fs/read_write.c:16"]);
    assert_eq!(first(&vfs_read)["exported"], true);
    let helper = run(&database, &["def", "rw_helper"]);
    assert_eq!(first(&helper)["exported"], true);
    assert_eq!(first(&helper)["storage"], "static");
    assert_eq!(first(&helper)["signature"], "static int __init rw_helper(struct file *file)");
    let inline = run(&database, &["def", "file_pos"]);
    assert_eq!(first(&inline)["inline"], true);
    assert_eq!(first(&inline)["storage"], "static");
    assert_eq!(first(&run(&database, &["def", "sysctl_nr_open"]))["storage"], "extern");
    assert_eq!(first(&run(&database, &["def", "rw_init"]))["exported"], false);

    let references = run(&database, &["refs", "vfs_read"]);
    let exports = references["exports"].as_array().expect("exports");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0]["location"], "fs/read_write.c:25");
    assert_eq!(exports[0]["license"], "");
    let registrations = run(&database, &["refs", "rw_init"]);
    assert_eq!(registrations["registrations"][0]["macro"], "module_init");
    assert_eq!(registrations["registrations"][0]["location"], "fs/read_write.c:58");
    let namespaced = run(&database, &["refs", "rw_helper"]);
    assert_eq!(namespaced["exports"][0]["namespace"], "FIXTURE_NS");
    assert_eq!(namespaced["exports"][0]["license"], "GPL");
}

#[test]
fn definitions_of_every_kind() {
    let (_directory, database) = built();
    let kinds = |name: &str| brief(&run(&database, &["def", name]));
    assert_eq!(kinds("file_operations"), ["file_operations struct include/linux/fs.h:22"]);
    assert_eq!(kinds("rw_hint"), ["rw_hint enum include/linux/fs.h:14"]);
    assert_eq!(kinds("HINT_LONG"), ["HINT_LONG enumerator include/linux/fs.h:17"]);
    assert_eq!(first(&run(&database, &["def", "HINT_LONG"]))["signature"], "HINT_LONG = HINT_SHORT + 1 (enum rw_hint)");
    assert_eq!(kinds("loff_t"), ["loff_t typedef include/linux/types.h:5"]);
    assert_eq!(kinds("callback_t"), ["callback_t typedef include/linux/types.h:6"]);
    assert_eq!(kinds("FMODE_READ"), ["FMODE_READ macro include/linux/fs.h:10"]);
    assert_eq!(kinds("MAX_RW_COUNT"), ["MAX_RW_COUNT macro_function include/linux/fs.h:11"]);
    assert_eq!(first(&run(&database, &["def", "MAX_RW_COUNT"]))["signature"], "#define MAX_RW_COUNT(x, y) ((x) < (y) ? (x) : (y))");
    assert_eq!(kinds("rw_lock"), ["rw_lock variable fs/read_write.c:5"]);
    assert_eq!(first(&run(&database, &["def", "rw_lock"]))["signature"], "static DEFINE_MUTEX(rw_lock)");
    assert_eq!(kinds("rw_counter"), ["rw_counter variable fs/read_write.c:6"]);
    assert_eq!(kinds("rw_fops"), ["rw_fops variable fs/read_write.c:7"]);
    assert_eq!(kinds("BTRFS_FS"), ["BTRFS_FS kconfig fs/btrfs/Kconfig:3"]);
    assert_eq!(kinds("CONFIG_BTRFS_FS"), ["BTRFS_FS kconfig fs/btrfs/Kconfig:3"]);
    assert_eq!(kinds("entry_SYSCALL_64"), ["entry_SYSCALL_64 function arch/x86/entry/entry.S:3"]);
    assert_eq!(kinds("File systems"), ["File systems menu fs/Kconfig:2"]);
}

#[test]
fn refs_callers_and_callees() {
    let (_directory, database) = built();
    let callers = run(&database, &["callers", "vfs_read"]);
    let rows: Vec<String> = callers["results"].as_array().expect("results").iter().map(|row| format!("{} {}", row["caller"].as_str().unwrap_or(""), row["location"].as_str().unwrap_or(""))).collect();
    assert_eq!(rows, ["btrfs_ctree_search fs/btrfs/ctree.c:5", "vfs_read_fop fs/read_write.c:13", "ksys_read fs/read_write.c:49"]);

    let callees = run(&database, &["callees", "vfs_read"]);
    let names: Vec<&str> = callees["results"].as_array().expect("results").iter().map(|row| row["callee"].as_str().unwrap_or("")).collect();
    assert_eq!(names, ["fs_min", "rw_helper", "file_pos"]);
    assert_eq!(callees["results"][0]["defined"]["kind"], "macro_function");
    assert_eq!(callees["results"][1]["defined"]["location"], "fs/read_write.c:27");

    let references = run(&database, &["refs", "vfs_read_fop"]);
    assert_eq!(references["results"][0]["in"], "rw_fops");
    assert_eq!(references["results"][0]["references"], "function");
    assert_eq!(references["results"][0]["location"], "fs/read_write.c:8");
    let fields: Vec<String> = run(&database, &["refs", "loff_t"])["results"].as_array().expect("results").iter().map(|row| format!("{} {}", row["in"].as_str().unwrap_or(""), row["location"].as_str().unwrap_or(""))).collect();
    assert_eq!(fields, ["vfs_read fs/read_write.c:16", "file include/linux/fs.h:33"]);
}

#[test]
fn includes_resolve_against_the_tree() {
    let (_directory, database) = built();
    let answer = run(&database, &["includes", "include/linux/fs.h"]);
    let rows: Vec<(String, Option<String>)> = answer["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| (row["target"].as_str().unwrap_or("").to_owned(), row["resolved"].as_str().map(str::to_owned)))
        .collect();
    assert_eq!(
        rows,
        [
            ("linux/types.h".to_owned(), Some("include/linux/types.h".to_owned())),
            ("linux/magic.h".to_owned(), Some("include/uapi/linux/magic.h".to_owned())),
            ("asm/x.h".to_owned(), Some("arch/x86/include/asm/x.h".to_owned())),
            ("linux/missing.h".to_owned(), None),
            ("fs_local.h".to_owned(), Some("include/linux/fs_local.h".to_owned())),
        ]
    );
    let by = run(&database, &["included-by", "include/linux/fs.h"]);
    let files: Vec<&str> = by["results"].as_array().expect("results").iter().map(|row| row["file"].as_str().unwrap_or("")).collect();
    assert_eq!(files, ["fs/btrfs/ctree.h", "fs/btrfs/super.c", "fs/idioms.c", "fs/read_write.c"]);
    let suffix = run(&database, &["includes", "linux/fs.h"]);
    assert_eq!(suffix["file"], "include/linux/fs.h");
    let kconfig = run(&database, &["includes", "fs/Kconfig"]);
    assert_eq!(first(&kconfig)["resolved"], "fs/btrfs/Kconfig");
    assert_eq!(first(&kconfig)["kind"], "source");
    assert_eq!(first(&run(&database, &["includes", "fs/btrfs/ctree.c"]))["resolved"], "fs/btrfs/ctree.h");
}

#[test]
fn file_lists_its_symbols_in_line_order() {
    let (_directory, database) = built();
    let answer = run(&database, &["file", "fs/read_write.c"]);
    assert_eq!(answer["file"]["language"], "c");
    assert_eq!(answer["file"]["lines"], 58);
    assert_eq!(answer["file"]["parse_errors"], 0);
    assert_eq!(
        brief(&answer),
        [
            "rw_lock variable fs/read_write.c:5",
            "rw_counter variable fs/read_write.c:6",
            "rw_fops variable fs/read_write.c:7",
            "vfs_read_fop function fs/read_write.c:11",
            "vfs_read function fs/read_write.c:16",
            "rw_helper function fs/read_write.c:27",
            "sys_read function fs/read_write.c:32",
            "compat_sys_read function fs/read_write.c:37",
            "sys_getpid function fs/read_write.c:42",
            "ksys_read function fs/read_write.c:47",
            "rw_init function fs/read_write.c:54",
        ]
    );
}

#[test]
fn config_follows_kconfig_and_kbuild() {
    let (_directory, database) = built();
    let answer = run(&database, &["config", "CONFIG_BTRFS_FS"]);
    assert_eq!(answer["found"], true);
    assert_eq!(answer["definitions"][0]["location"], "fs/btrfs/Kconfig:3");
    assert_eq!(answer["definitions"][0]["signature"], "tristate \"Btrfs filesystem support\"");
    assert_eq!(answer["depends_on"], json!([{ "expr": "FS_RW", "via": "direct", "location": "fs/btrfs/Kconfig:7" }]));
    let selects: Vec<&str> = answer["selects"].as_array().expect("selects").iter().map(|row| row["symbol"].as_str().unwrap_or("")).collect();
    assert_eq!(selects, ["LIBFS", "CRC32"]);
    let depended: Vec<&str> = answer["depended_on_by"].as_array().expect("depended_on_by").iter().map(|row| row["symbol"].as_str().unwrap_or("")).collect();
    assert_eq!(depended, ["BTRFS_FS_POSIX_ACL"]);

    let kbuild = answer["kbuild"].as_array().expect("kbuild");
    let targets: Vec<&str> = kbuild.iter().map(|row| row["target"].as_str().unwrap_or("")).collect();
    assert_eq!(targets, ["fs/btrfs/", "fs/btrfs/btrfs.o"]);
    assert_eq!(kbuild[0]["makefile"], "fs/Makefile:3");
    let parts: Vec<(&str, &str)> = kbuild[1]["parts"].as_array().expect("parts").iter().map(|part| (part["target"].as_str().unwrap_or(""), part["source"].as_str().unwrap_or(""))).collect();
    assert_eq!(parts, [("fs/btrfs/super.o", "fs/btrfs/super.c"), ("fs/btrfs/ctree.o", "fs/btrfs/ctree.c"), ("fs/btrfs/acl.o", "fs/btrfs/acl.c")]);
    assert_eq!(answer["sources"], json!(["fs/btrfs/super.c", "fs/btrfs/ctree.c", "fs/btrfs/acl.c"]));

    let rw = run(&database, &["config", "FS_RW"]);
    assert_eq!(rw["selects"][0]["symbol"], "LIBFS");
    assert_eq!(rw["implies"][0], json!({ "symbol": "FS_ACL", "condition": "BLOCK", "location": "fs/Kconfig:15" }));
    let defaults: Vec<&str> = rw["defaults"].as_array().expect("defaults").iter().map(|row| row["expr"].as_str().unwrap_or("")).collect();
    assert_eq!(defaults, ["y if SMP", "n"]);
    assert_eq!(rw["depends_on"][0]["expr"], "BLOCK && !UML");
    let libfs = run(&database, &["config", "LIBFS"]);
    let selected_by: Vec<&str> = libfs["selected_by"].as_array().expect("selected_by").iter().map(|row| row["symbol"].as_str().unwrap_or("")).collect();
    assert_eq!(selected_by, ["BTRFS_FS", "FS_RW"]);

    let acl = run(&database, &["config", "BTRFS_FS_POSIX_ACL"]);
    assert_eq!(acl["kbuild"][0]["target"], "fs/btrfs/acl.o");
    assert_eq!(acl["kbuild"][0]["parent"], "fs/btrfs/btrfs.o");
    assert_eq!(acl["sources"], json!(["fs/btrfs/acl.c"]));
}

#[test]
fn search_matches_prefixes_and_globs() {
    let (_directory, database) = built();
    assert_eq!(brief(&run(&database, &["search", "btrfs_"])), ["btrfs_acl_get function fs/btrfs/acl.c:3", "btrfs_ctree_search function fs/btrfs/ctree.c:3", "btrfs_super_init function fs/btrfs/super.c:3"]);
    assert_eq!(brief(&run(&database, &["search", "*_read"])), ["compat_sys_read function fs/read_write.c:37", "idiom_read function fs/idioms.c:16", "ksys_read function fs/read_write.c:47", "sys_read function fs/read_write.c:32", "vfs_read function fs/read_write.c:16"]);
    let limited = run(&database, &["search", "btrfs_", "--limit", "2"]);
    assert_eq!(limited["count"], 2);
    assert_eq!(limited["truncated"], true);
}

#[test]
fn path_finds_the_shortest_call_chain() {
    let (_directory, database) = built();
    let answer = run(&database, &["path", "sys_read", "file_pos"]);
    assert_eq!(answer["found"], true);
    let names: Vec<&str> = answer["results"].as_array().expect("results").iter().map(|row| row["name"].as_str().unwrap_or("")).collect();
    assert_eq!(names, ["sys_read", "ksys_read", "vfs_read", "file_pos"]);
    assert_eq!(answer["results"][3]["called_at"], "fs/read_write.c:21");
    assert_eq!(run(&database, &["path", "file_pos", "sys_read"])["found"], false);
    assert_eq!(run(&database, &["path", "sys_read", "file_pos", "--depth", "2"])["found"], false);
    assert_eq!(run(&database, &["path", "sys_read", "nowhere"])["found"], false);
}

#[test]
fn unknown_names_are_empty_not_errors() {
    let (_directory, database) = built();
    for query in [["def", "nothing_here"], ["refs", "nothing_here"], ["callers", "nothing_here"], ["callees", "nothing_here"], ["includes", "no/such.h"], ["included-by", "no/such.h"], ["file", "no/such.c"], ["search", "zzz_"]] {
        let answer = run(&database, &query);
        assert_eq!(answer["ok"], true, "{query:?}");
        assert_eq!(answer["count"], 0, "{query:?}");
        assert_eq!(answer["results"], json!([]), "{query:?}");
    }
    let config = run(&database, &["config", "CONFIG_NOT_A_SYMBOL"]);
    assert_eq!(config["ok"], true);
    assert_eq!(config["found"], false);
}

#[test]
fn missing_and_corrupt_databases_are_errors() {
    let directory = tempfile::tempdir().expect("tempdir");
    let missing = directory.path().join("absent.db");
    let error = graph::cli(&["def".to_owned(), "x".to_owned()], &fixture_tree(), &missing).expect_err("missing database");
    assert!(error.to_string().contains("not found"), "{error}");
    let corrupt = directory.path().join("corrupt.db");
    std::fs::write(&corrupt, b"this is not a database, only text that is long enough to look like one".repeat(20)).expect("write");
    let error = graph::cli(&["stats".to_owned()], &fixture_tree(), &corrupt).expect_err("corrupt database");
    assert!(error.to_string().contains("not a usable graph database"), "{error}");
    let error = graph::cli(&["def".to_owned()], &fixture_tree(), &corrupt).expect_err("missing argument");
    assert!(error.to_string().contains("usable") || error.to_string().contains("missing"), "{error}");
}

#[test]
fn subdir_limits_the_build() {
    let directory = tempfile::tempdir().expect("tempdir");
    let database = directory.path().join("graph.db");
    let report = run(&database, &["build", "--subdir", "fs/btrfs"]);
    assert_eq!(report["counts"]["files"], 6);
    let excluded = run(&database, &["build", "--exclude", "fs", "--db", database.to_str().expect("utf8 path")]);
    assert_eq!(excluded["counts"]["files"], 7);
}

fn dump(database: &Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(database).expect("open");
    let mut lines = Vec::new();
    for table in ["names", "kinds", "files", "symbols", "includes", "calls", "refs", "exports", "registrations", "config_edges", "build_edges"] {
        let mut statement = connection.prepare(&format!("SELECT * FROM {table} ORDER BY rowid")).expect("prepare");
        let columns = statement.column_count();
        let mut rows = statement.query([]).expect("query");
        while let Some(row) = rows.next().expect("row") {
            let cells: Vec<String> = (0..columns).map(|index| format!("{:?}", row.get_ref(index).expect("cell"))).collect();
            lines.push(format!("{table}: {}", cells.join(" | ")));
        }
    }
    let mut statement = connection.prepare("SELECT key, value FROM meta WHERE key NOT IN ('built_at') ORDER BY key").expect("prepare meta");
    let mut rows = statement.query([]).expect("query meta");
    while let Some(row) = rows.next().expect("row") {
        lines.push(format!("meta: {} = {}", row.get::<_, String>(0).expect("key"), row.get::<_, String>(1).expect("value")));
    }
    lines
}

#[test]
fn two_builds_of_one_tree_have_the_same_logical_content() {
    let directory = tempfile::tempdir().expect("tempdir");
    let first = directory.path().join("first.db");
    let second = directory.path().join("second.db");
    run(&first, &["build", "--jobs", "1"]);
    run(&second, &["build", "--jobs", "8"]);
    let (left, right) = (dump(&first), dump(&second));
    assert!(left.len() > 100, "dump has {} lines", left.len());
    assert_eq!(left, right);
}
