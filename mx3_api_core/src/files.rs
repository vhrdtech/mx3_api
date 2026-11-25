//! Files kept outside the database.
//!
//! Files kept outside the database (WH-25 part and box photos, MAIL-9 raw mail): the DB holds a relative path
//! into a Syncthing share on the backend host, the bytes live as files. These are the export calls that move
//! what is still only in the DB out to files; both areas use the same request and report.

use serde::{Deserialize, Serialize};

/// One export pass over rows without a file or with a wrong one.
///
/// One pass of `POST /file/export` or `POST /mail/export`: rows with `id > after_id`, at most `limit` of them,
/// in id order. A row without a path gets its file written and the path set; a row with a path has its file
/// checked (present and the right size; the content hash too with `verify`) and rewritten from the DB copy when
/// it is missing or wrong. The client repeats with `after_id = last_id` until `left` is 0.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExportRequest {
    #[serde(default)]
    pub after_id: i64,
    /// 0 means the server's default
    #[serde(default)]
    pub limit: u32,
    /// Count and check, write nothing
    #[serde(default)]
    pub dry_run: bool,
    /// Re-hash files already exported
    #[serde(default)]
    pub verify: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExportReport {
    /// Rows looked at in this pass
    pub seen: u64,
    /// Written now (or would be, with `dry_run`)
    pub exported: u64,
    /// Already on disk and fine
    pub kept: u64,
    /// Was on disk but missing or wrong, written again from the DB copy
    pub rewritten: u64,
    /// On disk missing or wrong and no DB copy left to rewrite from
    pub missing: Vec<i64>,
    /// The DB copy's hash doesn't match the stored one; left alone
    pub mismatched: Vec<i64>,
    /// Rows with neither a path nor bytes
    pub no_bytes: Vec<i64>,
    /// Bytes written to files in this pass
    pub bytes_written: u64,
    /// The last id looked at; 0 when there was nothing
    pub last_id: i64,
    /// Rows after `last_id` still to look at
    pub left: u64,
    /// Where the files go (the backend host's path)
    pub dir: String,
}

impl ExportReport {
    pub fn add(&mut self, o: &Self) {
        self.seen += o.seen;
        self.exported += o.exported;
        self.kept += o.kept;
        self.rewritten += o.rewritten;
        self.missing.extend_from_slice(&o.missing);
        self.mismatched.extend_from_slice(&o.mismatched);
        self.no_bytes.extend_from_slice(&o.no_bytes);
        self.bytes_written += o.bytes_written;
        self.last_id = o.last_id;
        self.left = o.left;
        if self.dir.is_empty() {
            self.dir = o.dir.clone();
        }
    }

    /// Nothing missing, nothing mismatched, nothing without bytes.
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.mismatched.is_empty() && self.no_bytes.is_empty()
    }
}
