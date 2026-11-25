//! The library's folders on this computer. A share is the name of its root folder (`~/docs/datasheets` holds
//! share `datasheets`), so the catalog's `share/path` is the same on every computer and resolves here.

use std::path::{Path, PathBuf};

use crate::endpoints::library::{LibPath, valid_path, valid_share};

/// The roots of the library's shares on this computer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roots(Vec<PathBuf>);

impl Roots {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self(roots)
    }

    /// A `:`-separated list of folders, `~/` standing for `home`; empty entries are skipped.
    pub fn parse(list: &str, home: Option<&Path>) -> Self {
        Self(
            list.split(':')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(|r| match (r.strip_prefix("~/"), home) {
                    (Some(rest), Some(h)) => h.join(rest),
                    _ => PathBuf::from(r),
                })
                .collect(),
        )
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.0
    }

    /// The root of a share: the root folder named so.
    pub fn root(&self, share: &str) -> Option<&Path> {
        self.0
            .iter()
            .find(|r| r.file_name().is_some_and(|n| n == share))
            .map(PathBuf::as_path)
    }

    /// The file a catalog path names, when it is on this computer. A share or path that is not well formed names
    /// nothing.
    pub fn resolve(&self, at: &LibPath) -> Option<PathBuf> {
        if !valid_share(&at.share) || !valid_path(&at.path) {
            return None;
        }
        let abs = self.root(&at.share)?.join(&at.path);
        abs.is_file().then_some(abs)
    }

    /// [`Self::resolve`] of `share/path` as the library lists it.
    pub fn locate(&self, rel: &str) -> Option<PathBuf> {
        let (share, path) = rel.split_once('/')?;
        self.resolve(&LibPath {
            share: share.to_owned(),
            path: path.to_owned(),
        })
    }

    /// The catalog path of a file on this computer: below which root, and the path below it.
    pub fn relative(&self, file: &Path) -> Option<LibPath> {
        self.0.iter().find_map(|r| {
            let rest = file.strip_prefix(r).ok()?;
            let share = r.file_name()?.to_str()?.to_owned();
            let path = rest
                .components()
                .map(|c| c.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()?
                .join("/");
            (valid_share(&share) && valid_path(&path)).then_some(LibPath { share, path })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_resolve_below_their_roots() {
        let dir = std::env::temp_dir().join(format!("mx3-api-roots-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("datasheets/Maker")).unwrap();
        std::fs::write(dir.join("datasheets/Maker/p.pdf"), b"x").unwrap();
        let roots = Roots::parse(
            &format!(" ~/books::{}", dir.join("datasheets").display()),
            Some(Path::new("/home/u")),
        );
        assert_eq!(roots.dirs()[0], Path::new("/home/u/books"));
        assert_eq!(roots.dirs().len(), 2);
        let want = dir.join("datasheets/Maker/p.pdf");
        assert_eq!(roots.locate("datasheets/Maker/p.pdf"), Some(want.clone()));
        assert_eq!(roots.locate("datasheets/Maker/none.pdf"), None);
        assert_eq!(roots.locate("books/Maker/p.pdf"), None);
        assert_eq!(roots.locate("datasheets/../datasheets/Maker/p.pdf"), None);
        assert_eq!(
            roots.locate("datasheets/Maker"),
            None,
            "a folder is not a file"
        );
        let rel = roots.relative(&want).unwrap();
        assert_eq!(rel.rel(), "datasheets/Maker/p.pdf");
        assert_eq!(roots.relative(Path::new("/elsewhere/p.pdf")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
