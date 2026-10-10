use gdb_wal::WriteAheadLog;
use std::fs::OpenOptions;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_wal_lifecycle_and_replay() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("test.wal");

    // 1. Open new WAL and write entries
    {
        let wal = WriteAheadLog::open(&wal_path).unwrap();
        wal.append(1, b"CREATE VERTEX 1").unwrap();
        wal.append(2, b"CREATE VERTEX 2").unwrap();
        wal.append(3, b"CREATE EDGE 1 -> 2").unwrap();
    }

    // 2. Re-open existing WAL and verify replay
    {
        let wal = WriteAheadLog::open(&wal_path).unwrap();
        let entries = wal.replay().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, 1);
        assert_eq!(entries[0].1, b"CREATE VERTEX 1");
        assert_eq!(entries[1].0, 2);
        assert_eq!(entries[1].1, b"CREATE VERTEX 2");
        assert_eq!(entries[2].0, 3);
        assert_eq!(entries[2].1, b"CREATE EDGE 1 -> 2");

        // Append more entries to existing WAL
        wal.append(4, b"COMMIT").unwrap();
    }

    // 3. Re-verify replay contains the fourth entry
    {
        let wal = WriteAheadLog::open(&wal_path).unwrap();
        let entries = wal.replay().unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[3].0, 4);
        assert_eq!(entries[3].1, b"COMMIT");
    }
}

#[test]
fn test_wal_corruption_handling() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("corrupt.wal");

    {
        let wal = WriteAheadLog::open(&wal_path).unwrap();
        wal.append(1, b"VALID DATA").unwrap();
    }

    // Corrupt the WAL by appending garbage bytes
    {
        let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
        file.write_all(b"GARBAGE_TRAILING_BYTES").unwrap();
    }

    // Replay should detect corruption / invalid checksum
    let wal = WriteAheadLog::open(&wal_path).unwrap();
    let res = wal.replay();
    assert!(res.is_err(), "Replaying corrupted WAL should return error");
}

#[test]
fn test_wal_truncate_and_bad_magic() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("trunc.wal");

    let wal = WriteAheadLog::open(&wal_path).unwrap();
    wal.append(1, b"ENTRY 1").unwrap();
    wal.append(2, b"ENTRY 2").unwrap();
    assert_eq!(wal.replay().unwrap().len(), 2);

    wal.truncate_to_empty().unwrap();
    assert_eq!(wal.replay().unwrap().len(), 0);

    wal.append(3, b"NEW ENTRY").unwrap();
    assert_eq!(wal.replay().unwrap().len(), 1);

    // Bad magic bytes
    let bad_path = dir.path().join("bad_magic.wal");
    std::fs::write(&bad_path, b"NOPE_BAD_MAGIC_HEADER_TEST").unwrap();
    let bad_wal = WriteAheadLog::open(&bad_path).unwrap();
    assert!(bad_wal.replay().is_err());
}
