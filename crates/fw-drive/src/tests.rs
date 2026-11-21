use super::*;

fn drive(quota: Option<u64>) -> (tempfile::TempDir, Drive) {
    let dir = tempfile::tempdir().expect("tempdir");
    let d = Drive::open(dir.path(), quota).expect("open");
    (dir, d)
}

fn u(e: &str) -> User {
    User::new(e).expect("user")
}

fn blobs(dir: &tempfile::TempDir) -> usize {
    fs::read_dir(dir.path().join("blobs"))
        .unwrap()
        .flatten()
        .map(|d| fs::read_dir(d.path()).unwrap().count())
        .sum()
}

#[test]
fn my_drive_is_stable_and_per_user() {
    let (_d, d) = drive(None);
    let a = d.my_drive(&u("A@example.com")).unwrap();
    assert_eq!(a, d.my_drive(&u("a@example.com")).unwrap());
    assert_eq!(a.name, "My Drive");
    assert_ne!(a.id, d.my_drive(&u("b@example.com")).unwrap().id);
    assert!(matches!(
        d.get(&u("b@example.com"), &a.id),
        Err(Error::NotFound)
    ));
}

#[test]
fn upload_dedupes_content_and_lists_folders_first() {
    let (dir, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    let b = d.upload(&me, &root, "b.txt", None, &b"same"[..]).unwrap();
    let a = d.upload(&me, &root, "a.txt", None, &b"same"[..]).unwrap();
    d.create_folder(&me, &root, "zeta").unwrap();
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.mime, "text/plain");
    assert_eq!(blobs(&dir), 1, "identical content is stored once");
    let names: Vec<_> = d
        .list(&me, &root, Sort::Name, false)
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    assert_eq!(names, ["zeta", "a.txt", "b.txt"]);
    let (_, path) = d.content(&me, &a.id).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"same");
    assert_eq!(
        fs::read_dir(dir.path().join("tmp")).unwrap().count(),
        0,
        "no temp file left behind"
    );
}

#[test]
fn names_are_validated() {
    let (_d, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    assert!(matches!(
        d.create_folder(&me, &root, "  "),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        d.create_folder(&me, &root, "a/b"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(d.rename(&me, &root, "x"), Err(Error::Invalid(_))));
}

#[test]
fn sharing_is_inherited_and_role_limited() {
    let (_d, d) = drive(None);
    let (me, you) = (u("me@example.com"), u("you@example.com"));
    let root = d.my_drive(&me).unwrap().id;
    let team = d.create_folder(&me, &root, "Team").unwrap();
    let doc = d
        .upload(&me, &team.id, "plan.md", None, &b"# plan"[..])
        .unwrap();
    assert!(matches!(d.get(&you, &doc.id), Err(Error::NotFound)));

    d.share(&me, &team.id, "You@Example.com", Role::Reader)
        .unwrap();
    assert_eq!(d.get(&you, &doc.id).unwrap().name, "plan.md");
    assert!(matches!(
        d.rename(&you, &doc.id, "x.md"),
        Err(Error::Forbidden(Role::Writer))
    ));
    assert!(matches!(
        d.share(&you, &team.id, "c@example.com", Role::Reader),
        Err(Error::Forbidden(Role::Owner))
    ));
    assert_eq!(d.shared_with_me(&you).unwrap().len(), 1);
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
        .unwrap()
        .len(),
        1
    );

    d.share(&me, &team.id, "you@example.com", Role::Writer)
        .unwrap();
    let up = d
        .upload(&you, &team.id, "notes.txt", None, &b"hi"[..])
        .unwrap();
    assert_eq!(
        up.owner, "me@example.com",
        "the folder owner owns uploads, as in Drive"
    );
    d.unshare(&me, &team.id, "you@example.com").unwrap();
    assert!(matches!(d.get(&you, &doc.id), Err(Error::NotFound)));
}

#[test]
fn anyone_with_the_link_can_read_but_does_not_see_it_listed() {
    let (_d, d) = drive(None);
    let (me, stranger) = (u("me@example.com"), u("x@example.org"));
    let root = d.my_drive(&me).unwrap().id;
    let f = d
        .upload(&me, &root, "public.txt", None, &b"hello"[..])
        .unwrap();
    d.share(&me, &f.id, "*", Role::Reader).unwrap();
    assert_eq!(d.get(&stranger, &f.id).unwrap().id, f.id);
    assert!(d.shared_with_me(&stranger).unwrap().is_empty());
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
        .unwrap()
        .is_empty()
    );
}

#[test]
fn moving_a_folder_into_itself_is_refused() {
    let (_d, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    let a = d.create_folder(&me, &root, "a").unwrap();
    let b = d.create_folder(&me, &a.id, "b").unwrap();
    assert!(matches!(
        d.move_to(&me, &a.id, &b.id),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        d.move_to(&me, &a.id, &a.id),
        Err(Error::Invalid(_))
    ));
    let moved = d.move_to(&me, &b.id, &root).unwrap();
    assert_eq!(moved.parent.as_deref(), Some(root.as_str()));
}

#[test]
fn trash_restore_and_delete_forever_collect_blobs() {
    let (dir, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    let folder = d.create_folder(&me, &root, "old").unwrap();
    let inner = d
        .upload(&me, &folder.id, "x.bin", None, &b"unique bytes"[..])
        .unwrap();
    let keep = d
        .upload(&me, &root, "keep.bin", None, &b"kept"[..])
        .unwrap();

    d.trash_node(&me, &folder.id).unwrap();
    assert!(
        d.list(&me, &root, Sort::Name, false)
            .unwrap()
            .iter()
            .all(|n| n.id != folder.id)
    );
    assert!(
        d.get(&me, &inner.id).unwrap().trashed,
        "children are trashed with their folder"
    );
    assert_eq!(
        d.trash(&me).unwrap().len(),
        1,
        "only the trashed folder is listed"
    );
    assert!(
        matches!(d.delete_forever(&me, &keep.id), Err(Error::Invalid(_))),
        "must be in the trash first"
    );

    d.restore(&me, &folder.id).unwrap();
    assert!(!d.get(&me, &inner.id).unwrap().trashed);

    d.trash_node(&me, &folder.id).unwrap();
    assert_eq!(blobs(&dir), 2);
    assert_eq!(d.empty_trash(&me).unwrap(), 1);
    assert!(matches!(d.get(&me, &inner.id), Err(Error::NotFound)));
    assert_eq!(
        blobs(&dir),
        1,
        "the orphaned blob is removed, the shared one stays"
    );
}

#[test]
fn restore_goes_to_my_drive_when_the_old_folder_is_trashed() {
    let (_d, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    let folder = d.create_folder(&me, &root, "box").unwrap();
    let f = d.upload(&me, &folder.id, "f.txt", None, &b"f"[..]).unwrap();
    d.trash_node(&me, &f.id).unwrap();
    d.trash_node(&me, &folder.id).unwrap();
    let back = d.restore(&me, &f.id).unwrap();
    assert_eq!(back.parent.as_deref(), Some(root.as_str()));
    assert!(!back.trashed);
}

#[test]
fn stars_are_per_user() {
    let (_d, d) = drive(None);
    let (me, you) = (u("me@example.com"), u("you@example.com"));
    let root = d.my_drive(&me).unwrap().id;
    let f = d.upload(&me, &root, "s.txt", None, &b"s"[..]).unwrap();
    d.share(&me, &f.id, "you@example.com", Role::Reader)
        .unwrap();
    d.set_starred(&you, &f.id, true).unwrap();
    assert_eq!(d.starred(&you).unwrap().len(), 1);
    assert!(d.starred(&me).unwrap().is_empty());
    assert!(!d.get(&me, &f.id).unwrap().starred);
}

#[test]
fn quota_stops_uploads_and_reports_largest() {
    let (dir, d) = drive(Some(10));
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    d.upload(&me, &root, "six.bin", None, &b"123456"[..])
        .unwrap();
    let err = d
        .upload(&me, &root, "five.bin", None, &b"12345"[..])
        .unwrap_err();
    assert!(matches!(err, Error::QuotaExceeded { .. }), "{err}");
    assert_eq!(
        fs::read_dir(dir.path().join("tmp")).unwrap().count(),
        0,
        "a refused upload leaves nothing"
    );
    let q = d.quota(&me).unwrap();
    assert_eq!((q.used, q.limit), (6, Some(10)));
    assert_eq!(q.largest[0].name, "six.bin");
}

#[test]
fn search_filters_by_type_and_escapes_wildcards() {
    let (_d, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    d.upload(&me, &root, "photo.png", None, &b"p"[..]).unwrap();
    d.upload(&me, &root, "100%_done.txt", None, &b"t"[..])
        .unwrap();
    d.upload(&me, &root, "1000done.txt", None, &b"u"[..])
        .unwrap();
    d.create_folder(&me, &root, "photos").unwrap();
    let s = |text: &str, kind: Option<&str>| -> Vec<String> {
        d.search(
            &me,
            &Search {
                text: text.into(),
                kind: kind.map(str::to_owned),
                modified_after: None,
            },
            Sort::Name,
            false,
        )
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect()
    };
    assert_eq!(s("photo", None), ["photos", "photo.png"]);
    assert_eq!(s("photo", Some("image")), ["photo.png"]);
    assert_eq!(s("", Some("folder")), ["photos"]);
    assert_eq!(s("%_", None), ["100%_done.txt"]);
}

#[test]
fn copy_shares_the_blob() {
    let (dir, d) = drive(None);
    let me = u("me@example.com");
    let root = d.my_drive(&me).unwrap().id;
    let f = d.upload(&me, &root, "a.txt", None, &b"abc"[..]).unwrap();
    let c = d.copy(&me, &f.id, &root).unwrap();
    assert_eq!(c.name, "Copy of a.txt");
    assert_eq!(c.hash, f.hash);
    assert_eq!(blobs(&dir), 1);
}

#[test]
fn reopening_keeps_everything() {
    let dir = tempfile::tempdir().unwrap();
    let me = u("me@example.com");
    let id = {
        let d = Drive::open(dir.path(), None).unwrap();
        let root = d.my_drive(&me).unwrap().id;
        d.upload(&me, &root, "persist.txt", None, &b"x"[..])
            .unwrap()
            .id
    };
    let d = Drive::open(dir.path(), None).unwrap();
    assert_eq!(d.get(&me, &id).unwrap().name, "persist.txt");
}
