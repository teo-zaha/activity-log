/// One collection run. Unique for the whole life of a host, across runtime replacements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(pub(crate) u64);
