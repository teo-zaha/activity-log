/// A stable FNV-1a hash of config bytes; a missing file has the id of empty bytes. Used only
/// inside one process, never sent to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(u64);

impl ContentId {
    pub fn of(bytes: &[u8]) -> Self {
        Self(crate::utils::fnv1a(bytes.iter().copied()))
    }
}
