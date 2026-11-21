use super::*;
use crate::sqlite::{FsBlobs, SqliteSql};
use crate::store::MemBlobs;
use futures_util::StreamExt;

/// Stands in for D1: SQLite underneath, but no pragmas or interactive transactions, at most 100 bound
/// parameters and integers that fit a JavaScript number, so a rule that works here works on D1.
struct D1Double(SqliteSql);

fn d1_checks(sql: &str, params: &[Value]) {
    let l = sql.trim_start().to_ascii_lowercase();
    assert!(
        !["begin", "commit", "rollback", "pragma"]
            .iter()
            .any(|p| l.starts_with(p)),
        "D1 has no interactive transactions or pragmas: {sql}"
    );
    assert!(params.len() <= 100, "D1 binds at most 100 parameters");
    for p in params {
        if let Value::Int(i) = p {
            assert!(i.abs() < (1 << 53), "D1 integers are JavaScript numbers");
        }
    }
}

#[async_trait::async_trait]
impl Sql for D1Double {
    async fn query(&self, sql: &str, params: Vec<Value>) -> Result<Vec<store::Row>> {
        d1_checks(sql, &params);
        self.0.query(sql, params).await
    }
    async fn batch(&self, stmts: Vec<Stmt>) -> Result<()> {
        for s in &stmts {
            d1_checks(&s.sql, &s.params);
        }
        self.0.batch(stmts).await
    }
}

/// One drive plus probes into its blob store.
struct Fx {
    drive: Drive,
    count: Box<dyn Fn() -> usize + Send>,
    pending: Box<dyn Fn() -> usize + Send>,
    _dir: Option<tempfile::TempDir>,
}

/// Local files and SQLite, then the D1 and R2 stand-ins: every rule runs over both.
fn fixtures(quota: Option<u64>) -> Vec<Fx> {
    let dir = tempfile::tempdir().expect("tempdir");
    let blobs = Arc::new(FsBlobs::open(dir.path()).expect("blobs"));
    let local = Fx {
        drive: Drive::new(
            Arc::new(SqliteSql::open(dir.path().join("drive.db")).expect("db")),
            blobs.clone(),
            quota,
        ),
        count: Box::new({
            let b = blobs.clone();
            move || b.count()
        }),
        pending: Box::new(move || blobs.pending()),
        _dir: Some(dir),
    };
    let mem = MemBlobs::new();
    let edge = Fx {
        drive: Drive::new(
            Arc::new(D1Double(SqliteSql::memory().expect("db"))),
            Arc::new(mem.clone()),
            quota,
        ),
        count: Box::new(move || mem.len()),
        pending: Box::new(|| 0),
        _dir: None,
    };
    vec![local, edge]
}

async fn each<F, Fut>(quota: Option<u64>, test: F)
where
    F: Fn(Fx) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for fx in fixtures(quota) {
        test(fx).await;
    }
}

fn u(e: &str) -> User {
    User::new(e).expect("user")
}

async fn bytes_of(d: &Drive, n: &Node) -> Vec<u8> {
    let mut s = d.read(n, None).await.unwrap();
    let mut out = Vec::new();
    while let Some(c) = s.next().await {
        out.extend_from_slice(&c.unwrap());
    }
    out
}

#[tokio::test]
async fn my_drive_is_stable_and_per_user() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let a = d.my_drive(&u("A@example.com")).await.unwrap();
        assert_eq!(a, d.my_drive(&u("a@example.com")).await.unwrap());
        assert_eq!(a.name, "My Drive");
        assert_ne!(a.id, d.my_drive(&u("b@example.com")).await.unwrap().id);
        assert!(matches!(
            d.get(&u("b@example.com"), &a.id).await,
            Err(Error::NotFound)
        ));
    })
    .await;
}

#[tokio::test]
async fn upload_dedupes_content_and_lists_folders_first() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        let b = d
            .upload(&me, &root, "b.txt", None, &b"same"[..])
            .await
            .unwrap();
        let a = d
            .upload(&me, &root, "a.txt", None, &b"same"[..])
            .await
            .unwrap();
        d.create_folder(&me, &root, "zeta").await.unwrap();
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.mime, "text/plain");
        assert_eq!((fx.count)(), 1, "identical content is stored once");
        let names: Vec<_> = d
            .list(&me, &root, Sort::Name, false)
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, ["zeta", "a.txt", "b.txt"]);
        let node = d.content(&me, &a.id).await.unwrap();
        assert_eq!(bytes_of(d, &node).await, b"same");
        assert_eq!((fx.pending)(), 0, "no temp file left behind");
    })
    .await;
}

#[tokio::test]
async fn names_are_validated() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        assert!(matches!(
            d.create_folder(&me, &root, "  ").await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            d.create_folder(&me, &root, "a/b").await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            d.rename(&me, &root, "x").await,
            Err(Error::Invalid(_))
        ));
    })
    .await;
}

#[tokio::test]
async fn sharing_is_inherited_and_role_limited() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let (me, you) = (u("me@example.com"), u("you@example.com"));
        let root = d.my_drive(&me).await.unwrap().id;
        let team = d.create_folder(&me, &root, "Team").await.unwrap();
        let doc = d
            .upload(&me, &team.id, "plan.md", None, &b"# plan"[..])
            .await
            .unwrap();
        assert!(matches!(d.get(&you, &doc.id).await, Err(Error::NotFound)));

        d.share(&me, &team.id, "You@Example.com", Role::Reader)
            .await
            .unwrap();
        assert_eq!(d.get(&you, &doc.id).await.unwrap().name, "plan.md");
        assert!(matches!(
            d.rename(&you, &doc.id, "x.md").await,
            Err(Error::Forbidden(Role::Writer))
        ));
        assert!(matches!(
            d.share(&you, &team.id, "c@example.com", Role::Reader).await,
            Err(Error::Forbidden(Role::Owner))
        ));
        assert_eq!(d.shared_with_me(&you).await.unwrap().len(), 1);
        assert_eq!(
            d.search(
                &you,
                &Search {
                    text: "plan".into(),
                    ..Search::default()
                },
                Sort::Name,
                false
            )
            .await
            .unwrap()
            .len(),
            1
        );

        d.share(&me, &team.id, "you@example.com", Role::Writer)
            .await
            .unwrap();
        let up = d
            .upload(&you, &team.id, "notes.txt", None, &b"hi"[..])
            .await
            .unwrap();
        assert_eq!(
            up.owner, "me@example.com",
            "the folder owner owns uploads, as in Drive"
        );
        d.unshare(&me, &team.id, "you@example.com").await.unwrap();
        assert!(matches!(d.get(&you, &doc.id).await, Err(Error::NotFound)));
    })
    .await;
}

#[tokio::test]
async fn anyone_with_the_link_can_read_but_does_not_see_it_listed() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let (me, stranger) = (u("me@example.com"), u("x@example.org"));
        let root = d.my_drive(&me).await.unwrap().id;
        let f = d
            .upload(&me, &root, "public.txt", None, &b"hello"[..])
            .await
            .unwrap();
        d.share(&me, &f.id, "*", Role::Reader).await.unwrap();
        assert_eq!(d.get(&stranger, &f.id).await.unwrap().id, f.id);
        assert!(d.shared_with_me(&stranger).await.unwrap().is_empty());
        assert!(
            d.search(
                &stranger,
                &Search {
                    text: "public".into(),
                    ..Search::default()
                },
                Sort::Name,
                false
            )
            .await
            .unwrap()
            .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn moving_a_folder_into_itself_is_refused() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        let a = d.create_folder(&me, &root, "a").await.unwrap();
        let b = d.create_folder(&me, &a.id, "b").await.unwrap();
        assert!(matches!(
            d.move_to(&me, &a.id, &b.id).await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            d.move_to(&me, &a.id, &a.id).await,
            Err(Error::Invalid(_))
        ));
        let moved = d.move_to(&me, &b.id, &root).await.unwrap();
        assert_eq!(moved.parent.as_deref(), Some(root.as_str()));
    })
    .await;
}

#[tokio::test]
async fn trash_restore_and_delete_forever_collect_blobs() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        let folder = d.create_folder(&me, &root, "old").await.unwrap();
        let inner = d
            .upload(&me, &folder.id, "x.bin", None, &b"unique bytes"[..])
            .await
            .unwrap();
        let keep = d
            .upload(&me, &root, "keep.bin", None, &b"kept"[..])
            .await
            .unwrap();

        d.trash_node(&me, &folder.id).await.unwrap();
        assert!(
            d.list(&me, &root, Sort::Name, false)
                .await
                .unwrap()
                .iter()
                .all(|n| n.id != folder.id)
        );
        assert!(
            d.get(&me, &inner.id).await.unwrap().trashed,
            "children are trashed with their folder"
        );
        assert_eq!(
            d.trash(&me).await.unwrap().len(),
            1,
            "only the trashed folder is listed"
        );
        assert!(
            matches!(
                d.delete_forever(&me, &keep.id).await,
                Err(Error::Invalid(_))
            ),
            "must be in the trash first"
        );

        d.restore(&me, &folder.id).await.unwrap();
        assert!(!d.get(&me, &inner.id).await.unwrap().trashed);

        d.trash_node(&me, &folder.id).await.unwrap();
        assert_eq!((fx.count)(), 2);
        assert_eq!(d.empty_trash(&me).await.unwrap(), 1);
        assert!(matches!(d.get(&me, &inner.id).await, Err(Error::NotFound)));
        assert_eq!(
            (fx.count)(),
            1,
            "the orphaned blob is removed, the shared one stays"
        );
    })
    .await;
}

#[tokio::test]
async fn restore_goes_to_my_drive_when_the_old_folder_is_trashed() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        let folder = d.create_folder(&me, &root, "box").await.unwrap();
        let f = d
            .upload(&me, &folder.id, "f.txt", None, &b"f"[..])
            .await
            .unwrap();
        d.trash_node(&me, &f.id).await.unwrap();
        d.trash_node(&me, &folder.id).await.unwrap();
        let back = d.restore(&me, &f.id).await.unwrap();
        assert_eq!(back.parent.as_deref(), Some(root.as_str()));
        assert!(!back.trashed);
    })
    .await;
}

#[tokio::test]
async fn stars_are_per_user() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let (me, you) = (u("me@example.com"), u("you@example.com"));
        let root = d.my_drive(&me).await.unwrap().id;
        let f = d
            .upload(&me, &root, "s.txt", None, &b"s"[..])
            .await
            .unwrap();
        d.share(&me, &f.id, "you@example.com", Role::Reader)
            .await
            .unwrap();
        d.set_starred(&you, &f.id, true).await.unwrap();
        assert_eq!(d.starred(&you).await.unwrap().len(), 1);
        assert!(d.starred(&me).await.unwrap().is_empty());
        assert!(!d.get(&me, &f.id).await.unwrap().starred);
    })
    .await;
}

#[tokio::test]
async fn quota_stops_uploads_and_reports_largest() {
    each(Some(10), |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        d.upload(&me, &root, "six.bin", None, &b"123456"[..])
            .await
            .unwrap();
        let err = d
            .upload(&me, &root, "five.bin", None, &b"12345"[..])
            .await
            .unwrap_err();
        assert!(matches!(err, Error::QuotaExceeded { .. }), "{err}");
        assert_eq!((fx.pending)(), 0, "a refused upload leaves nothing");
        let q = d.quota(&me).await.unwrap();
        assert_eq!((q.used, q.limit), (6, Some(10)));
        assert_eq!(q.largest[0].name, "six.bin");
    })
    .await;
}

#[tokio::test]
async fn search_filters_by_type_and_escapes_wildcards() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        d.upload(&me, &root, "photo.png", None, &b"p"[..])
            .await
            .unwrap();
        d.upload(&me, &root, "100%_done.txt", None, &b"t"[..])
            .await
            .unwrap();
        d.upload(&me, &root, "1000done.txt", None, &b"u"[..])
            .await
            .unwrap();
        d.create_folder(&me, &root, "photos").await.unwrap();
        let mr = &me;
        let s = |text: &'static str, kind: Option<&'static str>| async move {
            d.search(
                mr,
                &Search {
                    text: text.into(),
                    kind: kind.map(str::to_owned),
                    modified_after: None,
                },
                Sort::Name,
                false,
            )
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.name)
            .collect::<Vec<_>>()
        };
        assert_eq!(s("photo", None).await, ["photos", "photo.png"]);
        assert_eq!(s("photo", Some("image")).await, ["photo.png"]);
        assert_eq!(s("", Some("folder")).await, ["photos"]);
        assert_eq!(s("%_", None).await, ["100%_done.txt"]);
    })
    .await;
}

#[tokio::test]
async fn copy_shares_the_blob() {
    each(None, |fx| async move {
        let fx = fx;
        let d = &fx.drive;
        let me = u("me@example.com");
        let root = d.my_drive(&me).await.unwrap().id;
        let f = d
            .upload(&me, &root, "a.txt", None, &b"abc"[..])
            .await
            .unwrap();
        let c = d.copy(&me, &f.id, &root).await.unwrap();
        assert_eq!(c.name, "Copy of a.txt");
        assert_eq!(c.hash, f.hash);
        assert_eq!((fx.count)(), 1);
    })
    .await;
}

#[tokio::test]
async fn reopening_keeps_everything() {
    let dir = tempfile::tempdir().unwrap();
    let me = u("me@example.com");
    let id = {
        let d = Drive::open(dir.path(), None).unwrap();
        let root = d.my_drive(&me).await.unwrap().id;
        d.upload(&me, &root, "persist.txt", None, &b"x"[..])
            .await
            .unwrap()
            .id
    };
    let d = Drive::open(dir.path(), None).unwrap();
    assert_eq!(d.get(&me, &id).await.unwrap().name, "persist.txt");
}
