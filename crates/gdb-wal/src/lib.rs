pub mod wal;

pub use wal::WriteAheadLog;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_append_and_replay() {
        let tmp_file = tempfile::NamedTempFile::new().unwrap();
        let path = tmp_file.path();

        let wal = WriteAheadLog::open(path).unwrap();
        wal.append(1, b"first entry").unwrap();
        wal.append(2, b"second entry").unwrap();

        let replayed = wal.replay().unwrap();
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].0, 1);
        assert_eq!(replayed[0].1, b"first entry");
        assert_eq!(replayed[1].0, 2);
        assert_eq!(replayed[1].1, b"second entry");
    }
}
