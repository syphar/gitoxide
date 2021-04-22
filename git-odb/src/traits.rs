use crate::{data, pack};
use git_object::mutable;
use std::io;

/// Describe the capability to write git objects into an object store.
pub trait Write {
    /// The error type used for all trait methods.
    ///
    /// _Note_ the default implementations require the `From<io::Error>` bound.
    type Error: std::error::Error + From<io::Error>;

    /// Write [`object`][mutable::Object] using the given kind of [`hash`][git_hash::Kind] into the database,
    /// returning id to reference it in subsequent reads.
    fn write(&self, object: &mutable::Object, hash: git_hash::Kind) -> Result<git_hash::ObjectId, Self::Error> {
        let mut buf = Vec::with_capacity(2048);
        object.write_to(&mut buf)?;
        self.write_stream(object.kind(), buf.len() as u64, buf.as_slice(), hash)
    }
    /// As [`write`][Write::write], but takes an [`object` kind][git_object::Kind] along with its encoded bytes.
    fn write_buf(
        &self,
        object: git_object::Kind,
        from: &[u8],
        hash: git_hash::Kind,
    ) -> Result<git_hash::ObjectId, Self::Error> {
        self.write_stream(object, from.len() as u64, from, hash)
    }
    /// As [`write`][Write::write], but takes an input stream.
    /// This is commonly used for writing blobs directly without reading them to memory first.
    fn write_stream(
        &self,
        kind: git_object::Kind,
        size: u64,
        from: impl io::Read,
        hash: git_hash::Kind,
    ) -> Result<git_hash::ObjectId, Self::Error>;
}

/// Describe how object can be located in an object store
///
/// ## Notes
///
/// Locate effectively needs [generic associated types][issue] to allow a trait for the returned object type.
/// Until then, we will have to make due with explicit types and give them the potentially added features we want.
///
/// [issue]: https://github.com/rust-lang/rust/issues/44265
pub trait Locate {
    /// The error returned by [`locate()`][Locate::locate()]
    type Error: std::error::Error + 'static;

    /// Find an object matching `id` in the database while placing its raw, undecoded data into `buffer`.
    /// A `pack_cache` can be used to speed up subsequent lookups, set it to [`pack::cache::Noop`] if the
    /// workload isn't suitable for caching.
    ///
    /// Returns `Some` object if it was present in the database, or the error that occurred during lookup or object
    /// retrieval.
    fn locate<'a>(
        &self,
        id: impl AsRef<git_hash::oid>,
        buffer: &'a mut Vec<u8>,
        pack_cache: &mut impl crate::pack::cache::DecodeEntry,
    ) -> Result<Option<data::Object<'a>>, Self::Error>;

    fn pack_entry(&self, location: &pack::bundle::Location) -> Option<PackEntry<'_>>;
}

///
pub struct PackEntry<'a> {
    data: &'a [u8],
}

mod locate_impls {
    use crate::{data::Object, pack, PackEntry};
    use git_hash::oid;
    use std::ops::Deref;

    impl<T> super::Locate for std::sync::Arc<T>
    where
        T: super::Locate,
    {
        type Error = T::Error;

        fn locate<'a>(
            &self,
            id: impl AsRef<oid>,
            buffer: &'a mut Vec<u8>,
            pack_cache: &mut impl pack::cache::DecodeEntry,
        ) -> Result<Option<Object<'a>>, Self::Error> {
            self.deref().locate(id, buffer, pack_cache)
        }

        fn pack_entry(&self, location: &pack::bundle::Location) -> Option<PackEntry<'_>> {
            self.deref().pack_entry(location)
        }
    }

    impl<T> super::Locate for Box<T>
    where
        T: super::Locate,
    {
        type Error = T::Error;

        fn locate<'a>(
            &self,
            id: impl AsRef<oid>,
            buffer: &'a mut Vec<u8>,
            pack_cache: &mut impl pack::cache::DecodeEntry,
        ) -> Result<Option<Object<'a>>, Self::Error> {
            self.deref().locate(id, buffer, pack_cache)
        }

        fn pack_entry(&self, location: &pack::bundle::Location) -> Option<PackEntry<'_>> {
            self.deref().pack_entry(location)
        }
    }
}
