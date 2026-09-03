//! The entry table: the module's ordinal-addressable code addresses.

/// One ordinal slot. Gaps in the ordinal space are represented by absent map
/// keys rather than by a variant, since the on-disk form encodes them as runs.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Entry {
    pub ordinal: u16,
    /// 1-based segment number, or 0xFE for a constant.
    pub segment: u16,
    pub offset: u16,
    pub flags: u8,
    /// A movable entry is reached through an `int 3Fh` thunk the loader
    /// patches, so its address can change as segments are shuffled.
    pub moveable: bool,
    /// Name from the resident or non-resident name table, if it has one.
    pub name: Option<String>,
    /// True when the name came from the resident table, meaning the loader
    /// keeps it in memory and `GetProcAddress` by name is cheap.
    pub resident: bool,
}

impl Entry {
    /// Bit 0 of the flags marks the entry as exported.
    pub fn is_exported(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Bit 1 marks an entry that uses the module's shared data segment.
    pub fn uses_shared_data(&self) -> bool {
        self.flags & 0x02 != 0
    }

    /// The high nibble is the count of words the entry expects on the stack,
    /// used by the loader when it builds the call thunk.
    pub fn stack_words(&self) -> u8 {
        self.flags >> 3
    }

    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => n.clone(),
            None => format!("ord_{}", self.ordinal),
        }
    }
}
