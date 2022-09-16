use crate::match_group::Item;
use crate::RefSpecRef;
use bstr::{BStr, BString, ByteSlice, ByteVec};
use git_hash::ObjectId;
use std::borrow::Cow;
use std::ops::Range;

/// A type keeping enough information about a ref-spec to be able to efficiently match it against multiple matcher items.
pub struct Matcher<'a> {
    pub(crate) lhs: Option<Needle<'a>>,
    pub(crate) rhs: Option<Needle<'a>>,
}

impl<'a> Matcher<'a> {
    /// Match `item` against this spec and return `(true, Some<rhs>)` to gain the other side of the match as configured, or `(true, None)`
    /// if there was no `rhs`.
    ///
    /// This may involve resolving a glob with an allocation, as the destination is built using the matching portion of a glob.
    pub fn matches_lhs(&self, item: Item<'_>) -> (bool, Option<Cow<'a, BStr>>) {
        match (self.lhs, self.rhs) {
            (Some(lhs), None) => (lhs.matches(item).is_match(), None),
            (Some(lhs), Some(rhs)) => lhs.matches(item).into_match_outcome(rhs, item),
            _ => todo!(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum Needle<'a> {
    FullName(&'a BStr),
    PartialName(&'a BStr),
    Glob { name: &'a BStr, asterisk_pos: usize },
    Object(ObjectId),
}

enum Match {
    /// There was no match.
    None,
    /// No additional data is provided as part of the match.
    Normal,
    /// The range of text to copy from the originating item name
    GlobRange(Range<usize>),
}

impl Match {
    fn is_match(&self) -> bool {
        !matches!(self, Match::None)
    }
    fn into_match_outcome<'a>(self, destination: Needle<'a>, item: Item<'_>) -> (bool, Option<Cow<'a, BStr>>) {
        let arg = match self {
            Match::None => return (false, None),
            Match::Normal => None,
            Match::GlobRange(range) => Some((range, item)),
        };
        (true, destination.to_bstr_replace(arg).into())
    }
}

impl<'a> Needle<'a> {
    #[inline]
    fn matches(&self, item: Item<'_>) -> Match {
        match self {
            Needle::FullName(name) => {
                if *name == item.full_ref_name {
                    Match::Normal
                } else {
                    Match::None
                }
            }
            Needle::PartialName(name) => {
                let mut buf = BString::from(Vec::with_capacity(128));
                for (base, append_head) in [
                    ("refs/", false),
                    ("refs/tags/", false),
                    ("refs/heads/", false),
                    ("refs/remotes/", false),
                    ("refs/remotes/", true),
                ] {
                    buf.clear();
                    buf.push_str(base);
                    buf.push_str(name);
                    if append_head {
                        buf.push_str("/HEAD");
                    }
                    if buf == item.full_ref_name {
                        return Match::Normal;
                    }
                }
                Match::None
            }
            Needle::Glob { name, asterisk_pos } => {
                match item.full_ref_name.get(..*asterisk_pos) {
                    Some(full_name_portion) if full_name_portion != name[..*asterisk_pos] => {
                        return Match::None;
                    }
                    None => return Match::None,
                    _ => {}
                };
                let tail = &name[*asterisk_pos + 1..];
                if !item.full_ref_name.ends_with(tail) {
                    return Match::None;
                }
                let end = item.full_ref_name.len() - tail.len();
                let end = item.full_ref_name[*asterisk_pos..end].find_byte(b'/').unwrap_or(end);
                Match::GlobRange(*asterisk_pos..end)
            }
            Needle::Object(id) => {
                if *id == item.target {
                    return Match::Normal;
                }
                match item.tag {
                    Some(tag) if tag == *id => Match::Normal,
                    _ => Match::None,
                }
            }
        }
    }

    fn to_bstr_replace(self, range: Option<(Range<usize>, Item<'_>)>) -> Cow<'a, BStr> {
        match (self, range) {
            (Needle::FullName(name), None) => Cow::Borrowed(name),
            (Needle::PartialName(name), None) => Cow::Owned({
                let mut base: BString = "refs/".into();
                if !(name.starts_with(b"tags/") || name.starts_with(b"remotes/")) {
                    base.push_str("heads/");
                }
                base.push_str(name);
                base
            }),
            (Needle::Glob { name, asterisk_pos }, Some((range, item))) => {
                let mut buf = Vec::with_capacity(name.len() + range.len() - 1);
                buf.push_str(&name[..asterisk_pos]);
                buf.push_str(&item.full_ref_name[range]);
                buf.push_str(&name[asterisk_pos + 1..]);
                Cow::Owned(buf.into())
            }
            (Needle::Object(id), None) => {
                let mut name = id.to_string();
                name.insert_str(0, "refs/heads/");
                Cow::Owned(name.into())
            }
            (Needle::Glob { .. }, None) => unreachable!("BUG: no range provided for glob pattern"),
            (_, Some(_)) => {
                unreachable!("BUG: range provided even though needle wasn't a glob. Globs are symmetric.")
            }
        }
    }

    pub fn to_bstr(self) -> Cow<'a, BStr> {
        self.to_bstr_replace(None)
    }
}

impl<'a> From<&'a BStr> for Needle<'a> {
    fn from(v: &'a BStr) -> Self {
        if let Some(pos) = v.find_byte(b'*') {
            Needle::Glob {
                name: v,
                asterisk_pos: pos,
            }
        } else if v.starts_with(b"refs/") || v == "HEAD" {
            Needle::FullName(v)
        } else if let Ok(id) = git_hash::ObjectId::from_hex(v) {
            Needle::Object(id)
        } else {
            Needle::PartialName(v)
        }
    }
}

impl<'a> From<RefSpecRef<'a>> for Matcher<'a> {
    fn from(v: RefSpecRef<'a>) -> Self {
        Matcher {
            lhs: v.src.map(Into::into),
            rhs: v.dst.map(Into::into),
        }
    }
}
